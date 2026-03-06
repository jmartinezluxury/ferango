# Ferango

A lightweight, open-source MongoDB GUI client for developers. Built with Tauri 2, Vue 3, and the official MongoDB Rust driver.

Inspired by Studio 3T and DBeaver — fast, cross-platform, and pleasant to use.

![CI](https://github.com/jmartinezluxury/ferango/actions/workflows/ci.yml/badge.svg)
![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)
![License](https://img.shields.io/badge/license-MIT-green)
![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%202-orange)

[![Buy Me A Coffee](https://www.buymeacoffee.com/assets/img/custom_images/orange_img.png)](https://www.buymeacoffee.com/jmartinezgx)

---

## Download

Pre-built binaries are available on the [Releases](https://github.com/jmartinezluxury/ferango/releases) page.

| Platform | Installer |
|---|---|
| macOS (Apple Silicon + Intel) | `.dmg` |
| Windows | `.msi` or `.exe` |
| Linux | `.AppImage` or `.deb` |

> **macOS note:** If Gatekeeper blocks the app, right-click it and choose **Open**, or run `xattr -cr /Applications/Ferango.app` in the terminal.

---

## Features

### Editor
- **MongoDB shell syntax** — write queries exactly as in mongosh: `db.getCollection("users").find({status: "active"})`
- **Monaco Editor** — VS Code's engine with JavaScript syntax highlighting and IntelliSense
- **Field autocomplete** — field names from the selected collection populate the autocomplete list
- **Snippets** — common patterns (find, aggregate, updateOne, etc.) available via autocomplete
- **Multi-tab** — open multiple scripts simultaneously, auto-saved on tab switch
- **Run at cursor** — `Ctrl+Enter` executes the statement under the cursor
- **Run selection** — select text then `Ctrl+Enter` to run only the selected statements
- **Run all** — execute every statement in the file with the **Run all** button
- **Format** — prettify the query with `Shift+Alt+F` or the Format button

### Results
- **Table / JSON / Tree views** — switch between views without re-executing the query
- **Collapsible JSON tree** — expand and collapse nested documents
- **Pagination** — navigate result pages using skip + limit without touching your query
- **Click to copy** — click any cell to copy its value to clipboard
- **Full document panel** — click a row to open the complete document in a side drawer
- **Inline edit** — double-click a cell to edit and save a field directly from Table view
- **Export** — download results as CSV or JSON

### Connection management
- **Connection tree** — browse connections > databases > collections in a sidebar
- **SSH tunnel** — connect to MongoDB through an SSH jump host (key or agent auth)
- **Connection groups** — organize connections into named folders
- **Import from Compass** — import connections from a MongoDB Compass `connections.json` export
- **OS keychain** — passwords stored in macOS Keychain / Windows DPAPI / libsecret (not plaintext)

### Collection actions (right-click menu)
- **Find all documents** — appends and executes `find({})` in the active editor
- **Insert document** — open a JSON editor modal and insert a new document
- **Collection stats** — document count, storage size, avg object size, index count
- **View indexes** — list all indexes with their keys, unique, and sparse flags
- **Infer schema** — sample 100 documents and detect field types and presence percentage
- **Create / Drop collection**
- **Drop database**
- **Copy URI**

### Script management
- Scripts are saved to `~/.ferango/scripts/<connection-name>/`
- Sorted by last modified date
- Browse, rename, and delete from the Scripts panel
- Export all scripts as a `.zip` archive

### Query history
- Every executed query is logged automatically per connection
- Browse history in the Scripts panel; click an entry to open it in a new tab
- Clear history per connection

### Appearance
- **Dark / light theme** — toggle from the settings panel (`⚙` button in the toolbar)
- **Font size** — adjust editor and UI font size from 10 to 24px
- **Keyboard shortcuts reference** — open with the `?` button in the toolbar

---

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+Enter` / `Cmd+Enter` | Run statement at cursor |
| Select text + `Ctrl+Enter` | Run selected statements |
| Run all button | Run all statements in file |
| `Shift+Alt+F` | Format document |
| `Ctrl+S` | Save script |
| `Ctrl+F` | Find in editor |
| `Ctrl+Z` | Undo |
| `Ctrl+Shift+Z` | Redo |
| `Ctrl+/` | Toggle line comment |
| `Escape` | Close document side panel |

---

## Supported operations

```
find()           findOne()        aggregate()
countDocuments() count()
insertOne()      insertMany()
updateOne()      updateMany()
deleteOne()      deleteMany()
drop()
```

Supports full MongoDB shell syntax including:

- `ObjectId("...")`, `ISODate("...")`, `new Date("...")`
- `NumberInt(n)`, `NumberLong(n)`, `NumberDecimal("n")`
- `BinData(...)`, `UUID("...")`
- Regex literals: `/pattern/flags`
- Chained modifiers: `.sort({})`, `.limit(n)`, `.skip(n)`
- Line comments: `// comment`

---

## Getting started

### Prerequisites

- [Node.js](https://nodejs.org/) 18+
- [pnpm](https://pnpm.io/) — `npm install -g pnpm`
- [Rust](https://rustup.rs/) stable toolchain
- **macOS:** Xcode Command Line Tools — `xcode-select --install`
- **Linux:** see [system dependencies](#linux-system-dependencies) below
- **Windows:** Microsoft C++ Build Tools (installed with Visual Studio or via `winget install Microsoft.VisualStudio.2022.BuildTools`)

### Install

```bash
git clone https://github.com/jmartinezluxury/ferango.git
cd ferango
pnpm install
```

### Run in development

```bash
pnpm tauri dev
```

### Build for production

```bash
pnpm tauri build
```

Output installers are written to `src-tauri/target/release/bundle/`.

### Linux system dependencies

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  libappindicator3-dev \
  librsvg2-dev \
  patchelf \
  libdbus-1-dev \
  libsecret-1-dev
```

---

## Data storage

All data is stored locally — nothing is sent to any server.

| Path | Contents |
|---|---|
| `~/.ferango/connections.json` | Saved connection configs (passwords stored in OS keychain, not here) |
| `~/.ferango/scripts/<conn>/` | Query scripts per connection |
| `~/.ferango/history/<conn-id>.jsonl` | Query history (newline-delimited JSON, append-only) |
| `~/.ferango/settings.json` | Theme, font size, last used database per connection |

---

## Tech stack

| Layer | Technology |
|---|---|
| Frontend | Vue 3 + Vite + TypeScript |
| Editor | Monaco Editor |
| State | Pinia |
| Backend | Rust (Tauri 2) |
| MongoDB | Official Rust driver (`mongodb` crate) |
| Credential storage | OS keychain (`keyring` crate) |
| Storage | Local filesystem (`~/.ferango/`) |

---

## Creating a release

Releases are published automatically by GitHub Actions when you push a version tag:

```bash
git tag v1.2.0
git push origin v1.2.0
```

This triggers the [release workflow](.github/workflows/release.yml), which:

1. Builds a **universal macOS binary** (Apple Silicon + Intel) on `macos-latest`
2. Builds a **Windows** `.msi` + NSIS installer on `windows-latest`
3. Builds a **Linux** `.AppImage` + `.deb` on `ubuntu-22.04`
4. Creates a **draft GitHub Release** with all artifacts attached

Review the draft, edit the release notes if needed, then publish.

### macOS code signing and notarization (optional)

To distribute a signed and notarized `.dmg`, set the following repository secrets in **Settings → Secrets and variables → Actions**:

| Secret | Description |
|---|---|
| `APPLE_CERTIFICATE` | Base64-encoded `.p12` Developer ID Application certificate |
| `APPLE_CERTIFICATE_PASSWORD` | Password for the `.p12` file |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | Your Apple ID email |
| `APPLE_PASSWORD` | App-specific password from [appleid.apple.com](https://appleid.apple.com) |
| `APPLE_TEAM_ID` | Your 10-character Apple Team ID |

Export your Developer ID certificate from Keychain Access, then encode it:

```bash
base64 -i certificate.p12 | pbcopy   # macOS — copies to clipboard
```

Paste the result as the `APPLE_CERTIFICATE` secret.

### Windows code signing (optional)

| Secret | Description |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | Base64-encoded private key (generated with `tauri signer generate`) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password for the key |

Generate a key pair:

```bash
pnpm tauri signer generate -w ~/.tauri/myapp.key
```

---

## Contributing

Pull requests are welcome. For major changes, open an issue first.

```bash
# Run dev mode
pnpm tauri dev

# Type-check frontend only
pnpm build

# Check Rust (no build)
cd src-tauri && cargo check

# Lint Rust
cd src-tauri && cargo clippy
```

The CI workflow runs `pnpm build` and `cargo clippy` on every push and pull request.

---

## License

MIT

---

[![Buy Me A Coffee](https://www.buymeacoffee.com/assets/img/custom_images/orange_img.png)](https://www.buymeacoffee.com/jmartinezgx)
