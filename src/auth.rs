use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub auth: AuthConfig,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum AuthConfig {
    OAuth {
        client_secret_path: PathBuf,
        token: OAuthToken,
    },
    ServiceAccount {
        key_file_path: PathBuf,
    },
    Adc,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OAuthToken {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64, // unix timestamp
}

pub fn config_dir() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?
        .join("gws-cli");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

pub async fn load_config() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        anyhow::bail!("Not authenticated. Run: gws auth init");
    }
    let contents = tokio::fs::read_to_string(&path).await?;
    let config: Config = serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse config at {}", path.display()))?;
    Ok(config)
}

pub async fn save_config(config: &Config) -> Result<()> {
    let path = config_path()?;
    let contents = serde_json::to_string_pretty(config)?;
    tokio::fs::write(&path, contents).await?;
    Ok(())
}

pub async fn get_token(scopes: &[&str]) -> Result<String> {
    let config = load_config().await?;
    match &config.auth {
        AuthConfig::OAuth { client_secret_path, token } => {
            let token = refresh_oauth_token(client_secret_path, token, scopes).await?;
            Ok(token.access_token)
        }
        AuthConfig::ServiceAccount { key_file_path } => {
            let token = get_service_account_token(key_file_path, scopes).await?;
            Ok(token)
        }
        AuthConfig::Adc => {
            let token = get_adc_token(scopes).await?;
            Ok(token)
        }
    }
}

async fn refresh_oauth_token(
    client_secret_path: &PathBuf,
    token: &OAuthToken,
    _scopes: &[&str],
) -> Result<OAuthToken> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    
    // Token valid for at least 60 seconds
    if token.expires_at > now + 60 {
        return Ok(token.clone());
    }

    let secret_contents = tokio::fs::read_to_string(client_secret_path).await?;
    let secret: ClientSecret = serde_json::from_str(&secret_contents)
        .with_context(|| "Failed to parse client_secret JSON")?;

    let params = [
        ("client_id", secret.get()?.client_id.as_str()),
        ("client_secret", secret.get()?.client_secret.as_str()),
        ("refresh_token", token.refresh_token.as_str()),
        ("grant_type", "refresh_token"),
    ];

    let client = reqwest::Client::new();
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Token refresh failed: {}", text);
    }

    let refresh_resp: RefreshTokenResponse = resp.json().await?;
    let new_token = OAuthToken {
        access_token: refresh_resp.access_token,
        refresh_token: token.refresh_token.clone(),
        expires_at: now + refresh_resp.expires_in as u64,
    };

    // Update stored config
    let mut config = load_config().await?;
    if let AuthConfig::OAuth { token: ref mut t, .. } = config.auth {
        *t = new_token.clone();
    }
    save_config(&config).await?;

    Ok(new_token)
}

async fn get_service_account_token(key_file_path: &PathBuf, scopes: &[&str]) -> Result<String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use serde_json::Value;

    let key_contents = tokio::fs::read_to_string(key_file_path).await?;
    let key_json: Value = serde_json::from_str(&key_contents)?;

    let client_email = key_json["client_email"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing client_email in service account key"))?;
    let private_key = key_json["private_key"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing private_key in service account key"))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    let claims = serde_json::json!({
        "iss": client_email,
        "sub": client_email,
        "scope": scopes.join(" "),
        "aud": "https://oauth2.googleapis.com/token",
        "iat": now,
        "exp": now + 3600,
    });

    let header = Header::new(Algorithm::RS256);
    let encoding_key = EncodingKey::from_rsa_pem(private_key.as_bytes())?;
    let jwt = encode(&header, &claims, &encoding_key)?;

    let client = reqwest::Client::new();
    let params = [
        ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
        ("assertion", jwt.as_str()),
    ];

    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Service account token request failed: {}", text);
    }

    let token_resp: TokenResponse = resp.json().await?;
    Ok(token_resp.access_token)
}

async fn get_adc_token(scopes: &[&str]) -> Result<String> {
    // Check GOOGLE_APPLICATION_CREDENTIALS env var
    if let Ok(path) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
        return get_service_account_token(&PathBuf::from(path), scopes).await;
    }

    // Try GCP metadata server
    let client = reqwest::Client::new();
    let url = format!(
        "http://169.254.169.254/computeMetadata/v1/instance/service-accounts/default/token?scopes={}",
        urlencoding::encode(&scopes.join(","))
    );
    let resp = client
        .get(&url)
        .header("Metadata-Flavor", "Google")
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("ADC failed: not running on GCP and GOOGLE_APPLICATION_CREDENTIALS not set");
    }

    let token_resp: TokenResponse = resp.json().await?;
    Ok(token_resp.access_token)
}

#[derive(Deserialize)]
struct ClientSecret {
    installed: Option<InstalledSecret>,
    web: Option<InstalledSecret>,
}

impl ClientSecret {
    fn get(&self) -> Result<&InstalledSecret> {
        self.installed
            .as_ref()
            .or(self.web.as_ref())
            .ok_or_else(|| anyhow::anyhow!("client_secret.json must contain 'installed' or 'web' key"))
    }
}

#[derive(Deserialize)]
struct InstalledSecret {
    client_id: String,
    client_secret: String,
    #[allow(dead_code)]
    redirect_uris: Vec<String>,
}

#[derive(Deserialize)]
struct RefreshTokenResponse {
    access_token: String,
    expires_in: i64,
    #[allow(dead_code)]
    token_type: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[allow(dead_code)]
    token_type: String,
    #[allow(dead_code)]
    expires_in: Option<i64>,
}

// Device flow for initial OAuth setup (TV and Limited Input device clients)
pub async fn start_device_flow(client_secret_path: &PathBuf, scopes: &[&str]) -> Result<DeviceFlowStart> {
    let secret_contents = tokio::fs::read_to_string(client_secret_path).await?;
    let secret: ClientSecret = serde_json::from_str(&secret_contents)?;

    let client = reqwest::Client::new();
    let scope_str = scopes.join(" ");
    let params = [
        ("client_id", secret.get()?.client_id.as_str()),
        ("scope", scope_str.as_str()),
    ];

    let resp = client
        .post("https://oauth2.googleapis.com/device/code")
        .form(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Device flow start failed: {}", text);
    }

    let start: DeviceFlowStart = resp.json().await?;
    Ok(start)
}

pub async fn poll_device_token(
    client_secret_path: &PathBuf,
    device_code: &str,
) -> Result<OAuthToken> {
    let secret_contents = tokio::fs::read_to_string(client_secret_path).await?;
    let secret: ClientSecret = serde_json::from_str(&secret_contents)?;

    let client = reqwest::Client::new();
    let params = [
        ("client_id", secret.get()?.client_id.as_str()),
        ("client_secret", secret.get()?.client_secret.as_str()),
        ("device_code", device_code),
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
    ];

    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        let err: DeviceFlowError = serde_json::from_str(&text).unwrap_or(DeviceFlowError {
            error: "unknown_error".to_string(),
            error_description: text,
        });
        anyhow::bail!("{}: {}", err.error, err.error_description);
    }

    let token_resp: DeviceTokenResponse = resp.json().await?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    Ok(OAuthToken {
        access_token: token_resp.access_token,
        refresh_token: token_resp.refresh_token,
        expires_at: now + token_resp.expires_in as u64,
    })
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DeviceFlowStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_url: String,
    pub expires_in: i64,
    pub interval: i64,
}

#[derive(Deserialize)]
struct DeviceTokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    #[allow(dead_code)]
    token_type: String,
}

#[derive(Deserialize)]
struct DeviceFlowError {
    error: String,
    error_description: String,
}

// Installed app flow (Desktop clients) — local redirect server
pub async fn start_installed_app_flow(client_secret_path: &PathBuf, scopes: &[&str]) -> Result<OAuthToken> {
    let secret_contents = tokio::fs::read_to_string(client_secret_path).await?;
    let secret: ClientSecret = serde_json::from_str(&secret_contents)?;
    let secret = secret.get()?;

    // Use fixed port and path matching Google Cloud Console config
    let listener = TcpListener::bind("127.0.0.1:8000").await
        .with_context(|| "Failed to bind localhost:8000 — is another process using it?")?;
    let redirect_uri = "http://localhost:8000/oauth2callback".to_string();

    let scope_str = scopes.join(" ");
    let state = format!("gws_{}", uuid());

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}&access_type=offline&prompt=consent",
        urlencoding::encode(&secret.client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(&scope_str),
        urlencoding::encode(&state)
    );

    // Print the URL for the agent/user to open
    let mut output = serde_json::Map::new();
    output.insert("flow".to_string(), serde_json::json!("installed_app"));
    output.insert("auth_url".to_string(), serde_json::json!(auth_url));
    output.insert("redirect_uri".to_string(), serde_json::json!(redirect_uri));
    output.insert("message".to_string(), serde_json::json!("Please open the auth_url in a browser and approve access. Waiting for redirect..."));
    println!("{}", serde_json::to_string_pretty(&output)?);

    // Accept the redirect
    let (mut stream, _) = listener.accept().await?;
    let mut reader = tokio::io::BufReader::new(&mut stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).await?;

    // Parse the query string from the request line
    let code = extract_code(&request_line)?;

    // Send a simple response
    let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body><h1>Authentication successful!</h1><p>You can close this window.</p></body></html>";
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;

    // Exchange code for token
    let client = reqwest::Client::new();
    let params = [
        ("code", code.as_str()),
        ("client_id", secret.client_id.as_str()),
        ("client_secret", secret.client_secret.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("grant_type", "authorization_code"),
    ];

    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Token exchange failed: {}", text);
    }

    let token_resp: DeviceTokenResponse = resp.json().await?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    Ok(OAuthToken {
        access_token: token_resp.access_token,
        refresh_token: token_resp.refresh_token,
        expires_at: now + token_resp.expires_in as u64,
    })
}

fn extract_code(request_line: &str) -> Result<String> {
    // request_line looks like: GET /callback?code=...&state=... HTTP/1.1
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        anyhow::bail!("Invalid request line");
    }
    let path = parts[1];
    let query_start = path.find('?').ok_or_else(|| anyhow::anyhow!("No query string in redirect"))?;
    let query = &path[query_start + 1..];
    
    for param in query.split('&') {
        let mut kv = param.splitn(2, '=');
        let key = kv.next().unwrap_or("");
        let value = kv.next().unwrap_or("");
        if key == "code" {
            return Ok(urlencoding::decode(value)?.to_string());
        }
        if key == "error" {
            anyhow::bail!("OAuth error: {}", urlencoding::decode(value)?.to_string());
        }
    }
    anyhow::bail!("No authorization code in redirect")
}

fn uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{:x}", now)
}
