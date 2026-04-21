use crate::client::GwsClient;
use crate::output::{exit_with_error, print_json};
use anyhow::{Context, Result};
use clap::{Subcommand, ValueEnum};
use serde_json::json;
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub enum DriveCommands {
    /// List files
    List {
        #[arg(long)]
        query: Option<String>,
        #[arg(long)]
        page_size: Option<i32>,
        #[arg(long)]
        parent_id: Option<String>,
    },
    /// Upload a file
    Upload {
        path: PathBuf,
        #[arg(long)]
        folder_id: Option<String>,
        #[arg(long)]
        name: Option<String>,
    },
    /// Download a file
    Download {
        id: String,
        #[arg(long, short)]
        output: PathBuf,
    },
    /// Create a folder
    Mkdir {
        name: String,
        #[arg(long)]
        parent_id: Option<String>,
    },
    /// Delete a file
    Delete { id: String },
    /// Share a file or folder (create permission)
    Share {
        id: String,
        #[arg(short, long, value_enum)]
        role: ShareRole,
        #[arg(short, long, value_enum)]
        type_: ShareType,
        /// Email address (required for user or group type)
        #[arg(long)]
        email: Option<String>,
        /// Return a shareable web link in the response
        #[arg(long)]
        link: bool,
    },
}

#[derive(Clone, ValueEnum, Debug)]
pub enum ShareRole {
    Owner,
    Writer,
    Commenter,
    Reader,
}

#[derive(Clone, ValueEnum, Debug)]
pub enum ShareType {
    User,
    Group,
    Domain,
    Anyone,
}

pub async fn handle(cmd: DriveCommands) -> Result<()> {
    let client = GwsClient::new(&["https://www.googleapis.com/auth/drive"]).await;
    let client = match client {
        Ok(c) => c,
        Err(e) => {
            exit_with_error("AUTH_ERROR", &e.to_string(), Some("Run: gws auth init"));
        }
    };

    match cmd {
        DriveCommands::List {
            query,
            page_size,
            parent_id,
        } => {
            let mut url = "https://www.googleapis.com/drive/v3/files".to_string();
            let mut params = vec!["fields=*".to_string()];
            if let Some(p) = page_size {
                params.push(format!("pageSize={}", p));
            }
            let mut q_parts = Vec::new();
            if let Some(ref q) = query {
                q_parts.push(q.clone());
            }
            if let Some(parent) = parent_id {
                q_parts.push(format!("'{}' in parents", parent));
            }
            if !q_parts.is_empty() {
                params.push(format!("q={}", urlencoding::encode(&q_parts.join(" and "))));
            }
            url.push('?');
            url.push_str(&params.join("&"));
            let resp = client.get(&url).await?;
            print_json(resp);
        }
        DriveCommands::Upload {
            path,
            folder_id,
            name,
        } => {
            let file_name = name.unwrap_or_else(|| {
                path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unnamed".to_string())
            });
            let content = tokio::fs::read(&path).await
                .with_context(|| format!("Failed to read file: {}", path.display()))?;

            let mime_type = mime_guess::from_path(&path)
                .first()
                .map(|m| m.to_string())
                .unwrap_or_else(|| "application/octet-stream".to_string());

            let mut metadata = json!({
                "name": file_name,
                "mimeType": mime_type,
            });
            if let Some(folder) = folder_id {
                metadata["parents"] = json!([folder]);
            }

            let url = "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&fields=*";
            let resp = client.post_multipart(url, metadata, content).await?;
            print_json(resp);
        }
        DriveCommands::Download { id, output } => {
            let url = format!(
                "https://www.googleapis.com/drive/v3/files/{}?alt=media",
                id
            );
            let data = client.get_raw(&url).await?;
            tokio::fs::write(&output, data).await
                .with_context(|| format!("Failed to write file: {}", output.display()))?;
            print_json(json!({
                "success": true,
                "output": output.to_string_lossy().to_string(),
            }));
        }
        DriveCommands::Mkdir { name, parent_id } => {
            let mut body = json!({
                "name": name,
                "mimeType": "application/vnd.google-apps.folder",
            });
            if let Some(parent) = parent_id {
                body["parents"] = json!([parent]);
            }
            let resp = client
                .post(
                    "https://www.googleapis.com/drive/v3/files?fields=*",
                    Some(body),
                )
                .await?;
            print_json(resp);
        }
        DriveCommands::Delete { id } => {
            let url = format!("https://www.googleapis.com/drive/v3/files/{}", id);
            client.delete(&url).await?;
            print_json(json!({ "success": true, "deleted": id }));
        }
        DriveCommands::Share {
            id,
            role,
            type_,
            email,
            link,
        } => {
            let role_str = match role {
                ShareRole::Owner => "owner",
                ShareRole::Writer => "writer",
                ShareRole::Commenter => "commenter",
                ShareRole::Reader => "reader",
            };
            let type_str = match type_ {
                ShareType::User => "user",
                ShareType::Group => "group",
                ShareType::Domain => "domain",
                ShareType::Anyone => "anyone",
            };

            let mut body = json!({
                "role": role_str,
                "type": type_str,
            });
            if let Some(e) = email {
                body["emailAddress"] = json!(e);
            }

            let url = format!(
                "https://www.googleapis.com/drive/v3/files/{}/permissions",
                id
            );
            let resp = client.post(&url, Some(body)).await?;

            if link {
                // Fetch the file to get the webViewLink
                let file_url = format!(
                    "https://www.googleapis.com/drive/v3/files/{}?fields=webViewLink,webContentLink",
                    id
                );
                let file_resp = client.get(&file_url).await?;
                let mut combined = serde_json::Map::new();
                if let serde_json::Value::Object(m) = resp {
                    for (k, v) in m { combined.insert(k, v); }
                }
                if let serde_json::Value::Object(m) = file_resp {
                    for (k, v) in m { combined.insert(k, v); }
                }
                print_json(serde_json::Value::Object(combined));
            } else {
                print_json(resp);
            }
        }
    }
    Ok(())
}
