<p align="center">
  <img src="https://jmartinezluxury.github.io/ferango/ferango.svg" width="420" alt="Ferango" />
</p>

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

Get the latest version from the [**Releases page**](https://github.com/jmartinezluxury/ferango/releases/latest).

| Platform | Installer |
|---|---|
| macOS (Apple Silicon + Intel) | `.dmg` |
| Windows | `.msi` or `.exe` |
| Linux | `.AppImage` or `.deb` |

> **macOS note:** If Gatekeeper blocks the app, right-click it and choose **Open**, or run `xattr -cr /Applications/Ferango.app` in the terminal.

---

## What is Ferango?

Ferango is a desktop MongoDB client that lets you write queries in plain **mongosh syntax** — exactly as you would in a terminal — and see results in a clean, fast interface. No subscriptions, no cloud, no telemetry. Everything stays on your machine.

---

## Features

### Editor
- **mongosh syntax** — write queries like `db.getCollection("users").find({status: "active"})`
- **Monaco Editor** — VS Code's engine with JavaScript highlighting and IntelliSense
- **AI autocomplete** — inline ghost text suggestions powered by Ollama, OpenAI, or Claude (see below)
- **Field autocomplete** — field names from the active collection populate as you type
- **Snippets** — common patterns (find, aggregate, updateOne…) via autocomplete
- **Multi-tab** — open multiple scripts at once, auto-saved on tab switch
- **Run at cursor** — `Cmd+Enter` runs the statement under the cursor
- **Run selection** — select text, then `Cmd+Enter` to run only that
- **Run all** — execute every statement in the file with one click
- **Format** — prettify queries with `Shift+Alt+F`

### Results
- **Table / JSON / Tree views** — switch without re-running the query
- **Multi-statement results** — each statement gets its own result tab
- **Pagination** — navigate pages via skip + limit without editing your query
- **Click to copy** — click any cell to copy its value to clipboard
- **Full document panel** — click a row to open the complete document in a side panel
- **Export** — download results as CSV or JSON

### Connections
- **Connection tree** — browse connections → databases → collections in the sidebar
- **Connection groups** — organize connections into named folders
- **OS keychain** — passwords stored in macOS Keychain / Windows DPAPI / libsecret, never in plaintext

### Collection tools (right-click menu)
- **Find all documents** — runs `find({})` in the active editor
- **Insert document** — JSON editor modal to insert a new document
- **Collection stats** — document count, storage size, avg object size, index count
- **View indexes** — list all indexes with keys, unique, and sparse flags
- **Infer schema** — sample documents to detect field types and presence percentage
- **Create / Drop collection**
- **Drop database**

### Scripts & history
- Scripts saved to `~/.ferango/scripts/<connection-name>/`
- Sorted by last modified — browse, rename, and delete from the Scripts panel
- Every executed query is logged automatically per connection
- Click a history entry to open it in a new tab

### AI Autocomplete

Ferango includes built-in AI-powered code completion — type a comment or start a query and get inline suggestions.

- **Ollama (default)** — runs locally, free, no API key needed. Just install [Ollama](https://ollama.com) and pull a model (`ollama pull qwen2.5-coder:7b`)
- **OpenAI** — use your own API key with `gpt-4o-mini` or any compatible model
- **Claude** — use your own API key with `claude-haiku-4-5-20241022` or any Anthropic model

Configure in **Settings > AI Autocomplete**. API keys are stored in your OS keychain, never in config files. All AI requests are proxied through the local Rust backend — your keys never touch the renderer process.

> **Tip:** Write a comment like `// find users where name starts with J` and the AI will suggest the matching query.

### Appearance
- **Dark / light theme** — toggle from the toolbar
- **Font size** — adjust from 10 to 24px
- **Keyboard shortcuts reference** — open with the `?` button

---

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `Cmd+Enter` / `Ctrl+Enter` | Run statement at cursor |
| Select text + `Cmd+Enter` | Run selected statements |
| `Shift+Alt+F` | Format document |
| `Ctrl+S` | Save script |
| `Ctrl+F` | Find in editor |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / Redo |
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

Full mongosh syntax support including:

- `ObjectId("...")`, `ISODate("...")`, `new Date("...")`
- `NumberInt(n)`, `NumberLong(n)`, `NumberDecimal("n")`
- Regex literals: `/pattern/flags`
- Chained modifiers: `.sort({})`, `.limit(n)`, `.skip(n)`
- Line comments: `// comment`

---

## Data & privacy

All data is stored locally. Nothing is sent to any server except when AI autocomplete is enabled — in that case, the code context around your cursor is sent to your configured LLM provider (Ollama runs locally by default).

| Path | Contents |
|---|---|
| `~/.ferango/connections.json` | Saved connection configs (passwords in OS keychain, not here) |
| `~/.ferango/scripts/<conn>/` | Query scripts per connection |
| `~/.ferango/history/<conn-id>.jsonl` | Query history |
| `~/.ferango/settings.json` | Theme, font size, and preferences |

---

## Contributing

Pull requests are welcome. For major changes, please open an issue first to discuss what you'd like to change.

---

## License

MIT

---

[![Buy Me A Coffee](https://www.buymeacoffee.com/assets/img/custom_images/orange_img.png)](https://www.buymeacoffee.com/jmartinezgx)
