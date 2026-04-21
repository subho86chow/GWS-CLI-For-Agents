use anyhow::{Context, Result};
use reqwest::Response;
use serde::Serialize;
use serde_json::Value;

pub struct GwsClient {
    client: reqwest::Client,
    token: String,
}

impl GwsClient {
    pub async fn new(scopes: &[&str]) -> Result<Self> {
        let token = crate::auth::get_token(scopes).await?;
        Ok(Self {
            client: reqwest::Client::new(),
            token,
        })
    }

    pub async fn get(&self, url: &str) -> Result<Value> {
        let resp = self
            .client
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await?;
        handle_response(resp).await
    }

    pub async fn post(&self, url: &str, body: Option<Value>) -> Result<Value> {
        let mut req = self.client.post(url).bearer_auth(&self.token);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req.send().await?;
        handle_response(resp).await
    }

    pub async fn put(&self, url: &str, body: Value) -> Result<Value> {
        let resp = self
            .client
            .put(url)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await?;
        handle_response(resp).await
    }

    pub async fn delete(&self, url: &str) -> Result<Value> {
        let resp = self
            .client
            .delete(url)
            .bearer_auth(&self.token)
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await.unwrap_or(serde_json::Value::Null))
        } else {
            handle_response(resp).await
        }
    }

    pub async fn get_raw(&self, url: &str) -> Result<Vec<u8>> {
        let resp = self
            .client
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.bytes().await?.to_vec())
        } else {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("HTTP error: {}", text);
        }
    }

    pub async fn post_multipart(
        &self,
        url: &str,
        metadata: Value,
        file_content: Vec<u8>,
    ) -> Result<Value> {
        let boundary = "gws_cli_boundary_123456789";
        let metadata_part = format!(
            "--{}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{}\r\n",
            boundary,
            serde_json::to_string(&metadata)?
        );
        let file_part = format!(
            "--{}\r\nContent-Type: application/octet-stream\r\n\r\n",
            boundary
        );
        let end_part = format!("\r\n--{}--", boundary);

        let mut body = Vec::new();
        body.extend_from_slice(metadata_part.as_bytes());
        body.extend_from_slice(file_part.as_bytes());
        body.extend_from_slice(&file_content);
        body.extend_from_slice(end_part.as_bytes());

        let resp = self
            .client
            .post(url)
            .bearer_auth(&self.token)
            .header(
                "Content-Type",
                format!("multipart/related; boundary={}", boundary),
            )
            .body(body)
            .send()
            .await?;
        handle_response(resp).await
    }
}

async fn handle_response(resp: Response) -> Result<Value> {
    let status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();

    if status.is_success() {
        if body_text.trim().is_empty() {
            return Ok(Value::Null);
        }
        let value: Value = serde_json::from_str(&body_text)
            .with_context(|| format!("Failed to parse JSON response: {}", body_text))?;
        Ok(value)
    } else {
        let parsed: Result<Value, _> = serde_json::from_str(&body_text);
        let (code, message) = match parsed {
            Ok(val) => {
                let code = val["error"]["status"]
                    .as_str()
                    .or_else(|| val["error"].as_str())
                    .unwrap_or("API_ERROR");
                let message = val["error"]["message"]
                    .as_str()
                    .unwrap_or(&body_text);
                (code.to_string(), message.to_string())
            }
            Err(_) => ("API_ERROR".to_string(), body_text),
        };
        anyhow::bail!("{}|{}", code, message);
    }
}

#[derive(Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl ApiError {
    pub fn from_anyhow(err: &anyhow::Error) -> Option<Self> {
        let s = err.to_string();
        if let Some(pipe_pos) = s.find('|') {
            Some(ApiError {
                code: s[..pipe_pos].to_string(),
                message: s[pipe_pos + 1..].to_string(),
            })
        } else {
            None
        }
    }
}
