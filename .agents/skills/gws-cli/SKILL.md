# gws-cli

Agent-native CLI for Google Workspace APIs. JSON-only I/O. No human UI.

## Install

```bash
cargo install --path . --all-features
```

## Auth

OAuth (installed app flow) is the primary method. Requires `http://localhost:8000/oauth2callback` added as an **Authorized redirect URI** in your Google Cloud Console OAuth credentials.

```bash
gws auth init --method oauth --client-secret ./client_secret_*.json
```

Open the printed `auth_url` in a browser, approve, and the CLI captures the token automatically. Tokens are stored at `~/.config/gws-cli/config.json` and auto-refresh.

Other methods:
- Service account: `gws auth init --method service-account --key-file ./sa.json`
- ADC: `gws auth init --method adc`

## I/O Pattern

- **Stdout:** JSON (API response or structured result)
- **Stderr:** JSON error object with `code`, `message`, `suggestion`
- **Exit codes:** `0` success, `1` error, `2` auth, `3` permission/API

## Schema Discovery

```bash
gws schema
```

Outputs a JSON tree of all available commands, arguments, and types. Use this to discover capabilities dynamically.

## Services

### Gmail

```bash
gws gmail list --max-results 10 --query "is:unread"
gws gmail get <msg-id>
gws gmail send --to user@example.com --subject "Hello" --body "World"
gws gmail send --to user@example.com --subject "Report" --body "See attached" --attachment ./file.pdf
gws gmail trash <msg-id>
gws gmail labels
```

### Drive

```bash
gws drive list --query "name contains 'report'" --page-size 20
gws drive upload ./file.pdf --folder-id <id>
gws drive download <file-id> --output ./file.pdf
gws drive mkdir "New Folder" --parent-id <id>
gws drive delete <file-id>
gws drive share <file-id> --role reader --type anyone --link   # public link
gws drive share <file-id> --role writer --type user --email user@example.com
```

### Docs

```bash
gws docs create --title "My Doc"
gws docs get <doc-id>
echo '{"requests":[{"insertText":{"location":{"index":1},"text":"Hello"}}]}' | gws docs update <doc-id>
```

### Sheets

```bash
gws sheets create --title "My Sheet" --sheets "Sheet1,Sheet2"
gws sheets get <id> --range "Sheet1!A1:C10"
echo '[["a","b","c"],[1,2,3]]' | gws sheets update <id> --range "Sheet1!A1"
echo '[["d","e","f"]]' | gws sheets append <id> --range "Sheet1"
```

## Notes

- **Attachments:** Gmail `send` supports multiple attachments via `--attachment ./a.pdf,./b.csv`.
- **Drive share:** `--link` flag returns the `webViewLink` in the response.
- **Docs/Sheets updates:** Complex mutations accept JSON from stdin.
- **API responses:** Raw Google API JSON is returned unchanged.
