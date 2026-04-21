use crate::client::GwsClient;
use crate::output::{exit_with_error, print_json};
use anyhow::{Context, Result};
use clap::Subcommand;
use serde_json::json;
use tokio::io::{self, AsyncReadExt};

#[derive(Subcommand, Debug)]
pub enum DocsCommands {
    /// Get a document by ID
    Get {
        id: String,
        #[arg(long)]
        suggestions_view_mode: Option<String>,
    },
    /// Create a new document
    Create {
        #[arg(long)]
        title: String,
    },
    /// Update a document with batchUpdate requests (JSON from stdin)
    Update {
        id: String,
    },
}

pub async fn handle(cmd: DocsCommands) -> Result<()> {
    let client = GwsClient::new(&["https://www.googleapis.com/auth/documents"]).await;
    let client = match client {
        Ok(c) => c,
        Err(e) => {
            exit_with_error("AUTH_ERROR", &e.to_string(), Some("Run: gws auth init"));
        }
    };

    match cmd {
        DocsCommands::Get {
            id,
            suggestions_view_mode,
        } => {
            let mut url = format!("https://docs.googleapis.com/v1/documents/{}", id);
            if let Some(mode) = suggestions_view_mode {
                url.push_str(&format!("?suggestionsViewMode={}", mode));
            }
            let resp = client.get(&url).await?;
            print_json(resp);
        }
        DocsCommands::Create { title } => {
            let body = json!({ "title": title });
            let resp = client
                .post("https://docs.googleapis.com/v1/documents", Some(body))
                .await?;
            print_json(resp);
        }
        DocsCommands::Update { id } => {
            let mut stdin = io::stdin();
            let mut buf = String::new();
            stdin
                .read_to_string(&mut buf)
                .await
                .context("Failed to read batchUpdate requests from stdin")?;
            let requests: serde_json::Value = serde_json::from_str(&buf)
                .context("Invalid JSON in batchUpdate requests")?;
            let url = format!(
                "https://docs.googleapis.com/v1/documents/{}:batchUpdate",
                id
            );
            let resp = client.post(&url, Some(requests)).await?;
            print_json(resp);
        }
    }
    Ok(())
}
