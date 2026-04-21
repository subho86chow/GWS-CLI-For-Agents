use crate::client::GwsClient;
use crate::output::{exit_with_error, print_json};
use anyhow::{Context, Result};
use clap::Subcommand;
use serde_json::json;
use tokio::io::{self, AsyncReadExt};

#[derive(Subcommand, Debug)]
pub enum SheetsCommands {
    /// Get values from a range
    Get {
        id: String,
        #[arg(long)]
        range: String,
        #[arg(long)]
        major_dimension: Option<String>,
        #[arg(long)]
        value_render_option: Option<String>,
    },
    /// Update values in a range (values JSON from stdin)
    Update {
        id: String,
        #[arg(long)]
        range: String,
        #[arg(long)]
        major_dimension: Option<String>,
        #[arg(long)]
        value_input_option: Option<String>,
    },
    /// Append values to a range (values JSON from stdin)
    Append {
        id: String,
        #[arg(long)]
        range: String,
        #[arg(long)]
        major_dimension: Option<String>,
        #[arg(long)]
        value_input_option: Option<String>,
    },
    /// Create a new spreadsheet
    Create {
        #[arg(long)]
        title: String,
        #[arg(long)]
        sheets: Option<String>,
    },
}

pub async fn handle(cmd: SheetsCommands) -> Result<()> {
    let client = GwsClient::new(&["https://www.googleapis.com/auth/spreadsheets"]).await;
    let client = match client {
        Ok(c) => c,
        Err(e) => {
            exit_with_error("AUTH_ERROR", &e.to_string(), Some("Run: gws auth init"));
        }
    };

    match cmd {
        SheetsCommands::Get {
            id,
            range,
            major_dimension,
            value_render_option,
        } => {
            let encoded_range = urlencoding::encode(&range);
            let mut url = format!(
                "https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}",
                id, encoded_range
            );
            let mut params = vec![];
            if let Some(m) = major_dimension {
                params.push(format!("majorDimension={}", m));
            }
            if let Some(v) = value_render_option {
                params.push(format!("valueRenderOption={}", v));
            }
            if !params.is_empty() {
                url.push('?');
                url.push_str(&params.join("&"));
            }
            let resp = client.get(&url).await?;
            print_json(resp);
        }
        SheetsCommands::Update {
            id,
            range,
            major_dimension,
            value_input_option,
        } => {
            let mut stdin = io::stdin();
            let mut buf = String::new();
            stdin
                .read_to_string(&mut buf)
                .await
                .context("Failed to read values from stdin")?;
            let values: serde_json::Value = serde_json::from_str(&buf)
                .context("Invalid JSON in values")?;

            let mut body = json!({ "values": values });
            if let Some(m) = major_dimension {
                body["majorDimension"] = json!(m);
            }

            let encoded_range = urlencoding::encode(&range);
            let value_input = value_input_option.unwrap_or_else(|| "USER_ENTERED".to_string());
            let url = format!(
                "https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}?valueInputOption={}",
                id, encoded_range, value_input
            );
            let resp = client.put(&url, body).await?;
            print_json(resp);
        }
        SheetsCommands::Append {
            id,
            range,
            major_dimension,
            value_input_option,
        } => {
            let mut stdin = io::stdin();
            let mut buf = String::new();
            stdin
                .read_to_string(&mut buf)
                .await
                .context("Failed to read values from stdin")?;
            let values: serde_json::Value = serde_json::from_str(&buf)
                .context("Invalid JSON in values")?;

            let mut body = json!({ "values": values });
            if let Some(m) = major_dimension {
                body["majorDimension"] = json!(m);
            }

            let encoded_range = urlencoding::encode(&range);
            let value_input = value_input_option.unwrap_or_else(|| "USER_ENTERED".to_string());
            let url = format!(
                "https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}:append?valueInputOption={}&insertDataOption=INSERT_ROWS",
                id, encoded_range, value_input
            );
            let resp = client.post(&url, Some(body)).await?;
            print_json(resp);
        }
        SheetsCommands::Create { title, sheets } => {
            let mut body = json!({
                "properties": { "title": title },
            });
            if let Some(s) = sheets {
                let sheet_names: Vec<&str> = s.split(',').collect();
                let sheets_arr: Vec<serde_json::Value> = sheet_names
                    .into_iter()
                    .map(|name| json!({ "properties": { "title": name } }))
                    .collect();
                body["sheets"] = json!(sheets_arr);
            }
            let resp = client
                .post(
                    "https://sheets.googleapis.com/v4/spreadsheets",
                    Some(body),
                )
                .await?;
            print_json(resp);
        }
    }
    Ok(())
}
