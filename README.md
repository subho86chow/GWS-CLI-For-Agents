# gws-cli

Agent-native CLI for Google Workspace. Built for AI agents. Zero human UI.

- **JSON-only I/O** — stdout is always JSON, stderr is structured errors
- **Schema introspection** — `gws schema` dumps all commands for LLM tool-use
- **Modular** — Compile only the services you need via Cargo features
- **Three auth modes** — OAuth (installed app), Service Account, Application Default Credentials

---

## Table of Contents

- [Install](#install)
- [Authentication](#authentication)
  - [OAuth (installed app flow)](#oauth-installed-app-flow)
  - [Service Account](#service-account)
  - [Application Default Credentials (ADC)](#application-default-credentials-adc)
  - [Check auth status](#check-auth-status)
- [Schema Introspection](#schema-introspection)
- [Gmail](#gmail)
- [Drive](#drive)
- [Docs](#docs)
- [Sheets](#sheets)
- [Error Format](#error-format)
- [Exit Codes](#exit-codes)
- [Cargo Features](#cargo-features)

---

## Install

### From source

```bash
# Clone
git clone https://github.com/yourusername/gws-cli.git
cd gws-cli

# Install with all features
cargo install --path . --all-features

# Or pick only what you need
cargo install --path . --features gmail,drive
```

The `gws` binary is installed to `~/.cargo/bin`. Ensure this directory is in your `PATH`.

---

## Authentication

All API calls require authentication. The CLI stores tokens in the OS config directory:

- **macOS:** `~/Library/Application Support/gws-cli/config.json`
- **Linux:** `~/.config/gws-cli/config.json`
- **Windows:** `%APPDATA%/gws-cli/config.json`

### OAuth (installed app flow)

For Desktop-type OAuth clients (the standard `client_secret_*.json` from Google Cloud Console).

**Before you start:** You must configure the redirect URI in Google Cloud Console:

1. Go to [Google Cloud Console → APIs & Services → Credentials](https://console.cloud.google.com/apis/credentials)
2. Find your OAuth 2.0 Client ID (or create one: **Create Credentials → OAuth client ID → Desktop app**)
3. Click **Edit** (pencil icon)
4. Under **Authorized redirect URIs**, add:
   ```
   http://localhost:8000/oauth2callback
   ```
5. Click **Save**

> **Important:** The CLI uses `http://localhost:8000/oauth2callback` as the fixed redirect URI. If this is not added to your OAuth credentials, authentication will fail with an `invalid_client` or `redirect_uri_mismatch` error.
>
> **Note:** You do not need to add any "Authorized JavaScript origins" — those are only for browser-based web apps. This CLI only requires the redirect URI above.

Then run:

```bash
gws auth init --method oauth --client-secret ./client_secret_*.json
```

Output:
```json
{
  "flow": "installed_app",
  "auth_url": "https://accounts.google.com/o/oauth2/v2/auth?client_id=...",
  "redirect_uri": "http://localhost:8000/oauth2callback",
  "message": "Please open the auth_url in a browser and approve access. Waiting for redirect..."
}
```

Open the `auth_url` in a browser, approve the permissions, and the CLI automatically captures the redirect, exchanges the code for tokens, and stores the refresh token for future use.

**Scopes requested by default:**
- `gmail.modify`
- `drive`
- `documents`
- `spreadsheets`

You can override with `--scopes`:

```bash
gws auth init --method oauth --client-secret ./client_secret.json \
  --scopes https://www.googleapis.com/auth/gmail.readonly,https://www.googleapis.com/auth/drive.readonly
```

### Service Account

For headless/server environments. No browser interaction.

```bash
gws auth init --method service-account --key-file ./service-account.json
```

**Note:** Gmail and user Drive data often require [domain-wide delegation](https://developers.google.com/workspace/guides/create-credentials#optional_set_up_domain-wide_delegation_for_a_service_account) for service accounts.

### Application Default Credentials (ADC)

Uses `GOOGLE_APPLICATION_CREDENTIALS` environment variable or GCP metadata server.

```bash
export GOOGLE_APPLICATION_CREDENTIALS=/path/to/key.json
gws auth init --method adc
```

### Check auth status

```bash
gws auth status
```

---

## Schema Introspection

For LLM tool-use, dump the full command schema:

```bash
gws schema
```

This outputs a JSON tree of all available commands, arguments, and their types. An agent can parse this to discover capabilities without reading documentation.

---

## Gmail

### List messages

```bash
gws gmail list --max-results 10 --query "is:unread from:boss@company.com"
gws gmail list --label-ids INBOX,UNREAD
```

### Get a message

```bash
gws gmail get <message-id>
```

Get raw RFC2822 format:
```bash
gws gmail get <message-id> --raw
```

### Send a message

Plain text:
```bash
gws gmail send --to user@example.com --subject "Hello" --body "World"
```

With CC/BCC:
```bash
gws gmail send --to user@example.com --cc boss@example.com \
  --subject "Hello" --body "World"
```

With attachment(s):
```bash
gws gmail send --to user@example.com --subject "Report" --body "See attached" \
  --attachment ./report.pdf,./data.csv
```

### Trash a message

```bash
gws gmail trash <message-id>
```

### List labels

```bash
gws gmail labels
```

---

## Drive

### List files

```bash
gws drive list --page-size 20
gws drive list --query "name contains 'report'"
gws drive list --parent-id <folder-id>
```

### Upload a file

```bash
gws drive upload ./photo.jpg --name "vacation.jpg"
gws drive upload ./document.pdf --folder-id <folder-id> --name "renamed.pdf"
```

### Download a file

```bash
gws drive download <file-id> --output ./downloaded.pdf
```

### Create a folder

```bash
gws drive mkdir "Project Files" --parent-id <parent-folder-id>
```

### Delete a file

```bash
gws drive delete <file-id>
```

### Share a file or folder

**Public link (anyone with link can view):**
```bash
gws drive share <file-id> --role reader --type anyone --link
```

Output includes `webViewLink`:
```json
{
  "id": "anyoneWithLink",
  "role": "reader",
  "type": "anyone",
  "webViewLink": "https://docs.google.com/document/d/.../edit?usp=drivesdk"
}
```

**Share with a specific user:**
```bash
gws drive share <file-id> --role writer --type user --email teammate@example.com
```

**Roles:** `owner`, `writer`, `commenter`, `reader`
**Types:** `user`, `group`, `domain`, `anyone`

---

## Docs

### Create a document

```bash
gws docs create --title "My Document"
```

### Read a document

```bash
gws docs get <document-id>
```

### Update a document

Updates use the Google Docs `batchUpdate` API. Pass the requests as JSON via stdin:

```bash
echo '{
  "requests": [
    {
      "insertText": {
        "location": {"index": 1},
        "text": "Hello from the agent!"
      }
    }
  ]
}' | gws docs update <document-id>
```

See [Google Docs API batchUpdate reference](https://developers.google.com/docs/api/reference/rest/v1/documents/batchUpdate) for all available request types.

---

## Sheets

### Create a spreadsheet

```bash
gws sheets create --title "Sales Data" --sheets "Q1,Q2,Q3,Q4"
```

### Read values

```bash
gws sheets get <spreadsheet-id> --range "Sheet1!A1:D10"
```

### Update values

Pass values as a JSON 2D array via stdin:

```bash
echo '[
  ["Name", "Revenue", "Expenses"],
  ["Jan", 10000, 5000],
  ["Feb", 12000, 5500]
]' | gws sheets update <spreadsheet-id> --range "Sheet1!A1"
```

### Append values

```bash
echo '[["Mar", 11000, 4800]]' | gws sheets append <spreadsheet-id> --range "Sheet1"
```

---

## Error Format

All errors are JSON on stderr:

```json
{
  "error": {
    "code": "PERMISSION_DENIED",
    "message": "Insufficient permissions for Gmail",
    "suggestion": "Ensure the Gmail API is enabled and the token has the correct scope"
  }
}
```

Common error codes:

| Code | Meaning |
|---|---|
| `AUTH_ERROR` / `UNAUTHENTICATED` | Not authenticated or token expired |
| `PERMISSION_DENIED` | API not enabled or insufficient OAuth scope |
| `NOT_FOUND` | Resource does not exist |
| `INVALID_ARGUMENT` | Bad request (malformed ID, invalid range, etc.) |
| `API_ERROR` | Generic Google API error |

---

## Exit Codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | General error |
| `2` | Auth error (not authenticated, token refresh failed) |
| `3` | Permission / API error (HTTP 4xx/5xx from Google) |

---

## Cargo Features

| Feature | Service | Default? |
|---|---|---|
| `gmail` | Gmail API | No |
| `drive` | Drive API | No |
| `docs` | Docs API | No |
| `sheets` | Sheets API | No |
| `all` | All of the above | No |

Build with specific services:

```bash
cargo build --features gmail,drive
cargo build --all-features
```

When a feature is not compiled in, its subcommands do not appear in `--help` and are not available. This keeps the binary small and focused.

---

## Troubleshooting

### `invalid_client` or `redirect_uri_mismatch` during OAuth

**Cause:** The redirect URI `http://localhost:8000/oauth2callback` is not registered in your Google Cloud Console OAuth credentials.

**Fix:** Follow the [OAuth setup steps](#oauth-installed-app-flow) above to add the redirect URI.

### `PERMISSION_DENIED` or API errors

**Cause:** The Google Workspace API is not enabled for your project.

**Fix:** Enable the APIs you need in [Google Cloud Console → APIs & Services → Library](https://console.cloud.google.com/apis/library):
- Gmail API
- Google Drive API
- Google Docs API
- Google Sheets API

### Port 8000 already in use

**Cause:** Another process is using port 8000.

**Fix:** Kill the process using port 8000, or the CLI will fail with "Failed to bind localhost:8000".

```bash
# macOS/Linux
lsof -ti:8000 | xargs kill -9
```

---

## Design Principles

1. **Agent-native:** No colors, no tables, no prompts, no human formatting. JSON everywhere.
2. **Passthrough:** API responses return raw Google JSON so agents see the full documented shape.
3. **Composable:** Install only the services you need.
4. **Introspectable:** `gws schema` lets agents discover capabilities at runtime.
5. **Idempotent where possible:** Operations like `drive mkdir` create new resources (Google handles deduplication at the API level).

---

## License

MIT
