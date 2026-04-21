use crate::client::GwsClient;
use crate::output::{exit_with_error, print_json};
use anyhow::{Context, Result};
use clap::Subcommand;
use serde_json::json;
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub enum GmailCommands {
    /// List messages in the mailbox
    List {
        #[arg(long)]
        max_results: Option<i32>,
        #[arg(long, value_delimiter = ',')]
        label_ids: Option<Vec<String>>,
        #[arg(long)]
        query: Option<String>,
    },
    /// Get a message by ID
    Get {
        id: String,
        #[arg(long)]
        raw: bool,
    },
    /// Send a message (plain text or with attachments)
    Send {
        #[arg(long)]
        to: String,
        #[arg(long)]
        subject: String,
        #[arg(long)]
        body: String,
        #[arg(long)]
        cc: Option<String>,
        #[arg(long)]
        bcc: Option<String>,
        #[arg(long, value_delimiter = ',')]
        attachment: Option<Vec<PathBuf>>,
    },
    /// Move a message to trash
    Trash { id: String },
    /// List labels
    Labels,
}

pub async fn handle(cmd: GmailCommands) -> Result<()> {
    let client = GwsClient::new(&["https://www.googleapis.com/auth/gmail.modify"]).await;
    let client = match client {
        Ok(c) => c,
        Err(e) => {
            exit_with_error("AUTH_ERROR", &e.to_string(), Some("Run: gws auth init"));
        }
    };

    match cmd {
        GmailCommands::List {
            max_results,
            label_ids,
            query,
        } => {
            let mut url =
                "https://gmail.googleapis.com/gmail/v1/users/me/messages".to_string();
            let mut params = vec![];
            if let Some(m) = max_results {
                params.push(format!("maxResults={}", m));
            }
            if let Some(labels) = label_ids {
                for label in labels {
                    params.push(format!("labelIds={}", label));
                }
            }
            if let Some(q) = query {
                params.push(format!("q={}", urlencoding::encode(&q)));
            }
            if !params.is_empty() {
                url.push('?');
                url.push_str(&params.join("&"));
            }
            let resp = client.get(&url).await?;
            print_json(resp);
        }
        GmailCommands::Get { id, raw } => {
            let mut url = format!(
                "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}",
                id
            );
            if raw {
                url.push_str("?format=raw");
            }
            let resp = client.get(&url).await?;
            print_json(resp);
        }
        GmailCommands::Send {
            to,
            subject,
            body,
            cc,
            bcc,
            attachment,
        } => {
            let email_raw = build_email(&to, &subject, &body, cc.as_deref(), bcc.as_deref(), attachment).await?;
            let encoded = base64::Engine::encode(
                &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                email_raw.as_bytes(),
            );
            let payload = json!({ "raw": encoded });
            let resp = client
                .post(
                    "https://gmail.googleapis.com/gmail/v1/users/me/messages/send",
                    Some(payload),
                )
                .await?;
            print_json(resp);
        }
        GmailCommands::Trash { id } => {
            let url = format!(
                "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/trash",
                id
            );
            let resp = client.post(&url, None).await?;
            print_json(resp);
        }
        GmailCommands::Labels => {
            let resp = client
                .get("https://gmail.googleapis.com/gmail/v1/users/me/labels")
                .await?;
            print_json(resp);
        }
    }
    Ok(())
}

async fn build_email(
    to: &str,
    subject: &str,
    body: &str,
    cc: Option<&str>,
    bcc: Option<&str>,
    attachments: Option<Vec<PathBuf>>,
) -> Result<String> {
    let has_attachments = attachments.as_ref().map_or(false, |a| !a.is_empty());

    if !has_attachments {
        // Simple text/plain email
        let mut email = format!(
            "To: {}\r\nSubject: {}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=\"UTF-8\"\r\n\r\n{}",
            to, subject, body
        );
        if let Some(c) = cc {
            email = format!("Cc: {}\r\n{}", c, email);
        }
        if let Some(b) = bcc {
            email = format!("Bcc: {}\r\n{}", b, email);
        }
        return Ok(email);
    }

    // Multipart/mixed email with attachments
    let boundary = format!("gws_mixed_{}", uuid());
    let mut headers = vec![
        format!("To: {}", to),
        format!("Subject: {}", subject),
        "MIME-Version: 1.0".to_string(),
        format!("Content-Type: multipart/mixed; boundary=\"{}\"", boundary),
    ];
    if let Some(c) = cc {
        headers.push(format!("Cc: {}", c));
    }
    if let Some(b) = bcc {
        headers.push(format!("Bcc: {}", b));
    }

    let mut parts = vec![];

    // Text part
    parts.push(format!(
        "--{}\r\nContent-Type: text/plain; charset=\"UTF-8\"\r\n\r\n{}",
        boundary, body
    ));

    // Attachment parts
    for path in attachments.unwrap() {
        let data = tokio::fs::read(&path).await
            .with_context(|| format!("Failed to read attachment: {}", path.display()))?;
        let filename = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "attachment".to_string());
        let mime_type = mime_guess::from_path(&path)
            .first()
            .map(|m| m.to_string())
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let encoded = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &data,
        );
        // Chunk base64 into 76-char lines for MIME compliance
        let chunked: Vec<&str> = encoded.as_bytes()
            .chunks(76)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect();
        let chunked_str = chunked.join("\r\n");
        parts.push(format!(
            "--{}\r\nContent-Type: {}\r\nContent-Disposition: attachment; filename=\"{}\"\r\nContent-Transfer-Encoding: base64\r\n\r\n{}",
            boundary, mime_type, filename, chunked_str
        ));
    }

    parts.push(format!("--{}--", boundary));

    let email = format!("{}\r\n\r\n{}", headers.join("\r\n"), parts.join("\r\n"));
    Ok(email)
}

fn uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}", now)
}
