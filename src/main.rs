use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::process;

mod auth;
mod client;
mod docs;
mod drive;
#[cfg(feature = "gmail")]
mod gmail;
mod output;
mod schema;
#[cfg(feature = "sheets")]
mod sheets;

#[derive(Parser)]
#[command(name = "gws")]
#[command(about = "Agent-native CLI for Google Workspace")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Authentication commands
    Auth {
        #[command(subcommand)]
        command: AuthCommands,
    },
    /// Gmail operations
    #[cfg(feature = "gmail")]
    Gmail {
        #[command(subcommand)]
        command: gmail::GmailCommands,
    },
    /// Drive operations
    #[cfg(feature = "drive")]
    Drive {
        #[command(subcommand)]
        command: drive::DriveCommands,
    },
    /// Docs operations
    #[cfg(feature = "docs")]
    Docs {
        #[command(subcommand)]
        command: docs::DocsCommands,
    },
    /// Sheets operations
    #[cfg(feature = "sheets")]
    Sheets {
        #[command(subcommand)]
        command: sheets::SheetsCommands,
    },
    /// Output JSON schema of all available commands
    Schema,
}

#[derive(Subcommand)]
enum AuthCommands {
    /// Initialize authentication
    Init {
        #[arg(short, long, value_enum)]
        method: AuthMethod,
        #[arg(long)]
        client_secret: Option<PathBuf>,
        #[arg(long)]
        key_file: Option<PathBuf>,
        #[arg(long, value_delimiter = ',')]
        scopes: Option<Vec<String>>,
    },
    /// Check authentication status
    Status,
}

#[derive(Clone, ValueEnum)]
enum AuthMethod {
    Oauth,
    ServiceAccount,
    Adc,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli).await {
        let code = if let Some(api_err) = client::ApiError::from_anyhow(&e) {
            output::print_error(&api_err.code, &api_err.message, None);
            match api_err.code.as_str() {
                "UNAUTHENTICATED" | "AUTH_ERROR" => 2,
                "PERMISSION_DENIED" => 3,
                _ => 1,
            }
        } else {
            output::print_error("ERROR", &e.to_string(), None);
            1
        };
        process::exit(code);
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Commands::Auth { command } => match command {
            AuthCommands::Init {
                method,
                client_secret,
                key_file,
                scopes,
            } => {
                let default_scopes = vec![
                    "https://www.googleapis.com/auth/gmail.modify".to_string(),
                    "https://www.googleapis.com/auth/drive".to_string(),
                    "https://www.googleapis.com/auth/documents".to_string(),
                    "https://www.googleapis.com/auth/spreadsheets".to_string(),
                ];
                let scopes = scopes.unwrap_or(default_scopes);
                let scope_refs: Vec<&str> = scopes.iter().map(|s| s.as_str()).collect();

                match method {
                    AuthMethod::Oauth => {
                        let secret_path = client_secret
                            .ok_or_else(|| anyhow::anyhow!("--client-secret is required for OAuth"))?;
                        
                        // Try device flow first (preferred for agents)
                        let token = match auth::start_device_flow(&secret_path, &scope_refs).await {
                            Ok(start) => {
                                output::print_json(&start);

                                // Poll in background
                                let mut token: Option<auth::OAuthToken> = None;
                                let deadline = std::time::Instant::now()
                                    + std::time::Duration::from_secs(start.expires_in.max(0) as u64);
                                while std::time::Instant::now() < deadline {
                                    tokio::time::sleep(std::time::Duration::from_secs(
                                        start.interval.max(1) as u64,
                                    ))
                                    .await;
                                    match auth::poll_device_token(&secret_path, &start.device_code).await {
                                        Ok(t) => {
                                            token = Some(t);
                                            break;
                                        }
                                        Err(e) => {
                                            let msg = e.to_string();
                                            if msg.contains("authorization_pending")
                                                || msg.contains("slow_down")
                                            {
                                                continue;
                                            }
                                            return Err(e);
                                        }
                                    }
                                }
                                token.ok_or_else(|| anyhow::anyhow!("Device authorization timed out"))?
                            }
                            Err(e) if e.to_string().contains("TVs and Limited Input devices") => {
                                // Fall back to installed app flow for Desktop-type client secrets
                                auth::start_installed_app_flow(&secret_path, &scope_refs).await?
                            }
                            Err(e) => return Err(e),
                        };

                        let config = auth::Config {
                            auth: auth::AuthConfig::OAuth {
                                client_secret_path: secret_path,
                                token,
                            },
                        };
                        auth::save_config(&config).await?;
                        output::print_json(serde_json::json!({ "authenticated": true }));
                    }
                    AuthMethod::ServiceAccount => {
                        let key_path = key_file.ok_or_else(|| {
                            anyhow::anyhow!("--key-file is required for service-account auth")
                        })?;
                        let config = auth::Config {
                            auth: auth::AuthConfig::ServiceAccount {
                                key_file_path: key_path,
                            },
                        };
                        auth::save_config(&config).await?;
                        output::print_json(serde_json::json!({ "authenticated": true }));
                    }
                    AuthMethod::Adc => {
                        let config = auth::Config {
                            auth: auth::AuthConfig::Adc,
                        };
                        auth::save_config(&config).await?;
                        output::print_json(serde_json::json!({ "authenticated": true }));
                    }
                }
            }
            AuthCommands::Status => {
                match auth::load_config().await {
                    Ok(config) => {
                        output::print_json(serde_json::json!({
                            "authenticated": true,
                            "method": format!("{:?}", config.auth),
                        }));
                    }
                    Err(_) => {
                        output::print_json(serde_json::json!({
                            "authenticated": false,
                        }));
                    }
                }
            }
        },
        #[cfg(feature = "gmail")]
        Commands::Gmail { command } => gmail::handle(command).await?,
        #[cfg(feature = "drive")]
        Commands::Drive { command } => drive::handle(command).await?,
        #[cfg(feature = "docs")]
        Commands::Docs { command } => docs::handle(command).await?,
        #[cfg(feature = "sheets")]
        Commands::Sheets { command } => sheets::handle(command).await?,
        Commands::Schema => {
            let cmd = <Cli as clap::CommandFactory>::command();
            let schema = schema::build_schema(&cmd);
            output::print_json(schema);
        }
    }
    Ok(())
}
