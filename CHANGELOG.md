# Changelog

All notable changes to Ferango will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.2.0] — 2026-03-09

### Added
- **AI-powered inline autocomplete** — ghost text suggestions as you type, like GitHub Copilot
- Multi-provider support: **Ollama** (local, default), **OpenAI**, and **Claude** APIs
- AI settings panel in Settings modal — toggle on/off, select provider, configure endpoint, model, and API key
- API keys stored securely in OS keychain (never in config files)
- Fill-in-the-middle (FIM) support for compatible models (CodeLlama, DeepSeek Coder, StarCoder, Qwen2.5-Coder)
- Context-aware completions — sends active database, collection, and field names to the LLM
- Debounced requests (600ms) with automatic cancellation to avoid API spam
- "Test connection" button to verify AI provider reachability
- Auto-fill default endpoints and models when switching providers

## [0.1.1] — 2026-03-07

### Fixed
- Remove unnecessary `mut` on cursor variable (clippy `unused-mut`)
- Use `next_back()` instead of `last()` on `DoubleEndedIterator` (clippy `double-ended-iterator-last`)
- Replace `.map()` on `Option` returning unit with `if let` (clippy `option-map-unit-fn`)
- Remove needless borrows on `&args` passed to generic functions (clippy `needless-borrows-for-generic-args`)

## [0.1.0] — 2026-03-06

Initial public release.

### Added

#### Editor
- Monaco Editor (VS Code engine) with JavaScript syntax highlighting
- mongosh syntax support — write queries exactly as in a terminal
- `Cmd+Enter` / `Ctrl+Enter` to run the statement at cursor
- Run selected text with `Cmd+Enter` after selecting
- Run all statements in the file with one click
- Format document with `Shift+Alt+F`
- Field autocomplete — field names from the active collection populate as you type
- Snippets for common patterns: `find`, `aggregate`, `updateOne`, and more
- Multi-tab support — open multiple scripts at once, auto-saved on tab switch

#### Results
- Table, JSON, and Tree views — switch without re-running the query
- Multi-statement results — each statement gets its own result tab
- Pagination via skip + limit without editing your query
- Click any cell to copy its value to clipboard
- Full document side panel — click a row to see the complete document
- Export results as CSV or JSON

#### Connections
- Connection tree: browse connections → databases → collections in the sidebar
- Connection groups — organize connections into named folders
- OS keychain storage — passwords saved in macOS Keychain, Windows DPAPI, or libsecret

#### Collection tools (right-click menu)
- Find all documents
- Insert document via JSON editor modal
- Collection stats — document count, storage size, avg object size, index count
- View indexes — list all indexes with keys, unique, and sparse flags
- Infer schema — sample documents to detect field types and presence percentage
- Create and drop collections
- Drop database

#### Scripts & history
- Scripts saved to `~/.ferango/scripts/<connection-name>/`
- Browse, rename, and delete scripts from the Scripts panel
- Every executed query logged automatically per connection
- Click a history entry to open it in a new tab

#### Supported operations
- `find()`, `findOne()`, `aggregate()`
- `countDocuments()`, `count()`
- `insertOne()`, `insertMany()`
- `updateOne()`, `updateMany()`
- `deleteOne()`, `deleteMany()`
- `drop()`
- BSON types: `ObjectId()`, `ISODate()`, `new Date()`, `NumberInt()`, `NumberLong()`, `NumberDecimal()`
- Regex literals: `/pattern/flags`
- Chained modifiers: `.sort()`, `.limit()`, `.skip()`

#### Appearance
- Dark and light theme toggle
- Font size adjustable from 10 to 24px
- Keyboard shortcuts reference panel

[0.2.0]: https://github.com/jmartinezluxury/ferango/releases/tag/v0.2.0
[0.1.1]: https://github.com/jmartinezluxury/ferango/releases/tag/v0.1.1
[0.1.0]: https://github.com/jmartinezluxury/ferango/releases/tag/v0.1.0
