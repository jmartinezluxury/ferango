<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch, inject } from 'vue'
import * as monaco from 'monaco-editor'
import { useEditorStore } from '../stores/editor'
import type { StatementResult } from '../stores/editor'
import { useConnectionsStore } from '../stores/connections'
import { useSettingsStore } from '../stores/settings'
import { executeQuery, getFieldPaths, logQuery } from '../lib/tauri'

const editorStore = useEditorStore()
const connStore = useConnectionsStore()
const settingsStore = useSettingsStore()
const toast = inject<(msg: string, type?: 'success' | 'error' | 'info') => void>('toast')!

const containerEl = ref<HTMLDivElement | null>(null)
let monacoEditor: monaco.editor.IStandaloneCodeEditor | null = null
let autoSaveTimer: ReturnType<typeof setTimeout> | null = null
let completionDisposable: monaco.IDisposable | null = null

// ── Monaco setup ──────────────────────────────────────────────────────────────
onMounted(() => {
  if (!containerEl.value) return

  monacoEditor = monaco.editor.create(containerEl.value, {
    value: '// Select a collection from the tree or open a script\n',
    language: 'javascript',
    theme: 'vs-dark',
    fontSize: settingsStore.fontSize,
    fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
    fontLigatures: true,
    lineNumbers: 'on',
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
    wordWrap: 'off',
    tabSize: 2,
    automaticLayout: true,
    suggestOnTriggerCharacters: true,
    quickSuggestions: true,
    scrollbar: { vertical: 'auto', horizontal: 'auto', verticalScrollbarSize: 6, horizontalScrollbarSize: 6 },
  })

  // Register query snippets
  monaco.languages.registerCompletionItemProvider('javascript', {
    provideCompletionItems(model, position) {
      const word = model.getWordUntilPosition(position)
      const range = {
        startLineNumber: position.lineNumber, endLineNumber: position.lineNumber,
        startColumn: word.startColumn, endColumn: word.endColumn,
      }
      const snippets = [
        { label: 'find',           doc: 'Find documents',          text: 'db.getCollection("${1:collection}").find({\n\t${2:field}: ${3:value}\n})' },
        { label: 'findOne',        doc: 'Find one document',       text: 'db.getCollection("${1:collection}").findOne({\n\t${2:field}: ${3:value}\n})' },
        { label: 'aggregate',      doc: 'Aggregation pipeline',    text: 'db.getCollection("${1:collection}").aggregate([\n\t{ \\$match: { ${2:field}: ${3:value} } },\n\t{ \\$limit: 20 }\n])' },
        { label: 'updateOne',      doc: 'Update one document',     text: 'db.getCollection("${1:collection}").updateOne(\n\t{ ${2:filter} },\n\t{ \\$set: { ${3:field}: ${4:value} } }\n)' },
        { label: 'updateMany',     doc: 'Update many documents',   text: 'db.getCollection("${1:collection}").updateMany(\n\t{ ${2:filter} },\n\t{ \\$set: { ${3:field}: ${4:value} } }\n)' },
        { label: 'insertOne',      doc: 'Insert one document',     text: 'db.getCollection("${1:collection}").insertOne({\n\t${2:field}: ${3:value}\n})' },
        { label: 'insertMany',     doc: 'Insert many documents',   text: 'db.getCollection("${1:collection}").insertMany([\n\t{ ${2:field}: ${3:value} }\n])' },
        { label: 'deleteOne',      doc: 'Delete one document',     text: 'db.getCollection("${1:collection}").deleteOne({\n\t${2:filter}\n})' },
        { label: 'countDocuments', doc: 'Count documents',         text: 'db.getCollection("${1:collection}").countDocuments({\n\t${2:filter}\n})' },
      ]
      return {
        suggestions: snippets.map(s => ({
          label: s.label,
          kind: monaco.languages.CompletionItemKind.Snippet,
          documentation: s.doc,
          insertText: s.text,
          insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
          range,
        })),
      }
    },
  })

  // Ctrl+Enter / Cmd+Enter → run statement at cursor (or selection)
  monacoEditor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter, runAtCursor)

  // Shift+Alt+F → format document
  monacoEditor.addCommand(monaco.KeyMod.Shift | monaco.KeyMod.Alt | monaco.KeyCode.KeyF, formatQuery)

  // Auto-save on content change
  monacoEditor.onDidChangeModelContent(() => {
    const tab = editorStore.activeTab()
    if (!tab) return
    editorStore.setContent(monacoEditor!.getValue())
    // Debounce 2s
    if (autoSaveTimer) clearTimeout(autoSaveTimer)
    autoSaveTimer = setTimeout(() => editorStore.saveActive(), 2000)
  })
})

onBeforeUnmount(() => {
  completionDisposable?.dispose()
  monacoEditor?.dispose()
  if (autoSaveTimer) clearTimeout(autoSaveTimer)
})

// ── Sync font size from settings ──────────────────────────────────────────────
watch(() => settingsStore.fontSize, (size) => {
  monacoEditor?.updateOptions({ fontSize: size })
})

// ── External "insert + execute" trigger ───────────────────────────────────────
watch(() => editorStore.pendingExec, async (query) => {
  if (!query || !monacoEditor) return
  editorStore.pendingExec = null

  // If no tab is open, nothing to append to — just execute directly
  const current = editorStore.activeTab() ? monacoEditor.getValue() : ''
  const appended = (current.trimEnd() ? current.trimEnd() + '\n\n' : '') + query + '\n'
  monacoEditor.setValue(appended)
  editorStore.setContent(appended)

  // Scroll to the appended line
  const lineCount = monacoEditor.getModel()?.getLineCount() ?? 1
  monacoEditor.revealLine(lineCount)

  // Execute only the new statement (strip trailing semicolon for parser)
  const stmt = query.replace(/;\s*$/, '')
  await executeStatements([stmt])
})

// ── Sync editor content when tab changes ─────────────────────────────────────
watch(() => editorStore.activeTabIndex, () => {
  const tab = editorStore.activeTab()
  if (monacoEditor && tab) {
    monacoEditor.setValue(tab.content)
    monacoEditor.setScrollTop(0)
  } else if (monacoEditor && !tab) {
    monacoEditor.setValue('// Select a collection from the tree or open a script\n')
  }
})

// ── Autocomplete: update when collection changes ──────────────────────────────
watch(() => connStore.activeCollection, async (col) => {
  const conn = connStore.activeConn
  const db = connStore.activeDb
  if (!col || !conn || !db) return
  try {
    const fields = await getFieldPaths(conn, db, col)
    updateCompletions(fields)
  } catch { /* ignore */ }
})

function updateCompletions(fields: string[]) {
  completionDisposable?.dispose()
  completionDisposable = monaco.languages.registerCompletionItemProvider('javascript', {
    provideCompletionItems(model, position) {
      const word = model.getWordUntilPosition(position)
      const range = {
        startLineNumber: position.lineNumber,
        endLineNumber: position.lineNumber,
        startColumn: word.startColumn,
        endColumn: word.endColumn,
      }
      return {
        suggestions: fields.map(f => ({
          label: f,
          kind: monaco.languages.CompletionItemKind.Field,
          insertText: f,
          range,
        })),
      }
    },
  })
}

// ── Format query ──────────────────────────────────────────────────────────────
function formatQuery() {
  monacoEditor?.getAction('editor.action.formatDocument')?.run()
}

// ── Run query helpers ─────────────────────────────────────────────────────────
async function executeStatements(stmts: string[]) {
  const conn = connStore.activeConn
  const db = connStore.activeDb
  if (!conn) { toast('Select a connection first', 'error'); return }
  if (!db) { toast('Select a database first', 'error'); return }
  if (!stmts.length) { toast('No valid statement found', 'error'); return }

  editorStore.isExecuting = true
  const label = stmts.length === 1 ? 'Executing…' : `Executing ${stmts.length} statements…`
  toast(label, 'info')

  try {
    const items: StatementResult[] = []
    const isSinglePaginatable = stmts.length === 1
      && /\.(find|aggregate)\s*\(/.test(stmts[0])
      && !/.limit\s*\(/.test(stmts[0])
    for (let i = 0; i < stmts.length; i++) {
      let stmt = stmts[i]
      if (/\.(find|aggregate)\s*\(/.test(stmt) && !/.limit\s*\(/.test(stmt)) {
        stmt = `${stmt}.limit(${editorStore.queryLimit})`
      }
      const result = await executeQuery(conn, db, stmt)
      items.push({ label: `#${i + 1}`, result })
      if (conn.id) logQuery(conn.id, db, stmts[i], result.elapsed_ms).catch(() => {})
    }
    editorStore.setResults(items, isSinglePaginatable ? stmts[0] : '')
    const errors = items.filter(it => !it.result.success)
    if (errors.length) toast(`${errors.length} statement(s) failed`, 'error')
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    editorStore.isExecuting = false
  }
}

// Ctrl+Enter: run selection if present, otherwise run statement at cursor
async function runAtCursor() {
  if (!monacoEditor) return
  const sel = monacoEditor.getSelection()
  const selText = sel ? monacoEditor.getModel()?.getValueInRange(sel) : ''
  if (selText?.trim()) {
    // Run selected text
    const stmts = extractAllStatements(selText.trim()).filter(Boolean)
    await executeStatements(stmts)
    return
  }
  // No selection — find statement at cursor
  const model = monacoEditor.getModel()
  if (!model) return
  const pos = monacoEditor.getPosition()
  if (!pos) return
  const offset = model.getOffsetAt(pos)
  const text = model.getValue()
  const stmt = extractStatementAtCursor(text, offset).trim()
  if (!stmt) { toast('No statement at cursor', 'error'); return }
  await executeStatements([stmt])
}

// Run all: full file (or selection if present)
async function runAll() {
  if (!monacoEditor) return
  const sel = monacoEditor.getSelection()
  const selText = sel ? monacoEditor.getModel()?.getValueInRange(sel) : ''
  const raw = (selText?.trim() || monacoEditor.getValue() || '').trim()
  if (!raw) { toast('No query to execute', 'error'); return }
  const stmts = extractAllStatements(raw).filter(Boolean)
  await executeStatements(stmts)
}

function extractStatementAtCursor(text: string, offset: number): string {
  // Strip line comments but preserve offsets
  const src = text.replace(/\/\/[^\n]*/g, m => ' '.repeat(m.length))

  // Walk forward to cursor, tracking statement boundaries
  let start = 0
  let prevStart = 0  // start of the statement before the last ';'
  let lastSemi = -1  // position of the last ';' at depth 0 before cursor
  let depth = 0
  let inStr: string | null = null
  for (let i = 0; i < offset && i < src.length; i++) {
    const ch = src[i]
    if (inStr) {
      if (ch === inStr && src[i - 1] !== '\\') inStr = null
    } else if (ch === '"' || ch === "'" || ch === '`') {
      inStr = ch
    } else if (ch === '(' || ch === '{' || ch === '[') {
      depth++
    } else if (ch === ')' || ch === '}' || ch === ']') {
      depth--
    } else if (ch === ';' && depth === 0) {
      prevStart = start
      lastSemi = i
      start = i + 1
    }
  }

  // Walk forward from start to find next ';' at depth 0
  depth = 0
  inStr = null
  let end = src.length
  for (let i = start; i < src.length; i++) {
    const ch = src[i]
    if (inStr) {
      if (ch === inStr && src[i - 1] !== '\\') inStr = null
    } else if (ch === '"' || ch === "'" || ch === '`') {
      inStr = ch
    } else if (ch === '(' || ch === '{' || ch === '[') {
      depth++
    } else if (ch === ')' || ch === '}' || ch === ']') {
      depth--
    } else if (ch === ';' && depth === 0) {
      end = i
      break
    }
  }

  const result = text.slice(start, end).trim()
  // Cursor is right after a ';' → fall back to the statement that ended there
  if (!result && lastSemi >= 0) {
    return text.slice(prevStart, lastSemi).trim()
  }
  return result
}

function extractAllStatements(text: string): string[] {
  // Strip line comments
  const src = text.split('\n').map(l => l.replace(/\/\/.*$/, '')).join('\n')
  const stmts: string[] = []
  let depth = 0
  let inStr: string | null = null
  let cur = ''

  for (let i = 0; i < src.length; i++) {
    const ch = src[i]
    if (inStr) {
      cur += ch
      if (ch === inStr && src[i - 1] !== '\\') inStr = null
    } else if (ch === '"' || ch === "'" || ch === '`') {
      inStr = ch
      cur += ch
    } else if (ch === '(' || ch === '{' || ch === '[') {
      depth++; cur += ch
    } else if (ch === ')' || ch === '}' || ch === ']') {
      depth--; cur += ch
    } else if (ch === ';' && depth === 0) {
      const s = cur.trim()
      if (s) stmts.push(s)
      cur = ''
    } else {
      cur += ch
    }
  }
  const last = cur.trim()
  if (last) stmts.push(last)
  return stmts
}
</script>

<template>
  <div class="editor-root">
    <!-- Tab bar -->
    <div class="tabs-bar">
      <div class="tabs-list">
        <div
          v-for="(tab, i) in editorStore.tabs"
          :key="tab.script.path"
          :class="['tab', { active: i === editorStore.activeTabIndex }]"
          @click="editorStore.switchTab(i)"
        >
          <span class="tab-name">{{ tab.script.name }}{{ tab.modified ? ' ●' : '' }}</span>
          <button class="tab-close" title="Close" @click.stop="editorStore.closeTab(i)">✕</button>
        </div>
      </div>
    </div>

    <!-- Editor toolbar -->
    <div class="editor-toolbar">
      <button
        class="run-btn"
        :disabled="editorStore.isExecuting"
        title="Run statement at cursor, or selection (Ctrl+Enter)"
        @click="runAtCursor"
      >
        {{ editorStore.isExecuting ? '◌' : '▶' }} Run
      </button>
      <button
        class="run-all-btn"
        :disabled="editorStore.isExecuting"
        title="Run all statements in file (or selection)"
        @click="runAll"
      >
        ▶▶ Run all
      </button>
      <span class="editor-hint">Ctrl+Enter: cursor statement · select text for partial run</span>
      <span class="toolbar-spacer" />
      <button class="btn-ghost" style="font-size:11px" title="Format document (Shift+Alt+F)" @click="formatQuery">Format</button>
      <button class="btn-ghost" style="font-size:11px" @click="editorStore.saveActive()">Save</button>
    </div>

    <!-- Monaco container -->
    <div ref="containerEl" class="monaco-container" />
  </div>
</template>

<style scoped>
.editor-root { display: flex; flex-direction: column; height: 100%; overflow: hidden; background: var(--bg); }

/* Tabs */
.tabs-bar {
  display: flex; align-items: center;
  background: var(--bg-sidebar); border-bottom: 1px solid var(--border);
  height: 30px; flex-shrink: 0; overflow-x: auto; overflow-y: hidden;
}
.tabs-list { display: flex; height: 100%; }
.tab {
  display: flex; align-items: center; gap: 6px;
  padding: 0 10px; font-size: 11px; cursor: pointer;
  border-right: 1px solid var(--border); white-space: nowrap;
  color: var(--text-dim); background: transparent;
  transition: background 0.1s;
}
.tab:hover { background: var(--bg-hover); }
.tab.active { background: var(--bg); color: var(--text); border-bottom: 2px solid var(--accent); }
.tab-name { max-width: 120px; overflow: hidden; text-overflow: ellipsis; }
.tab-close {
  font-size: 10px; background: transparent; color: var(--text-muted);
  border-radius: 2px; padding: 1px 3px;
  opacity: 0; transition: opacity 0.1s;
}
.tab:hover .tab-close { opacity: 1; }
.tab-close:hover { background: var(--bg-hover); color: var(--red); }

/* Toolbar */
.editor-toolbar {
  display: flex; align-items: center; gap: 8px;
  padding: 4px 8px; background: var(--bg-card);
  border-bottom: 1px solid var(--border); flex-shrink: 0;
}
.run-btn {
  background: var(--accent); color: #0d1b2a;
  font-weight: 700; font-size: 11px;
  padding: 3px 10px; border-radius: var(--radius);
}
.run-btn:hover:not(:disabled) { background: var(--accent-dim); color: var(--text); }
.run-btn:disabled { opacity: 0.5; cursor: not-allowed; }
.run-all-btn {
  background: transparent; color: var(--accent);
  font-weight: 600; font-size: 11px;
  padding: 3px 10px; border-radius: var(--radius);
  border: 1px solid var(--accent);
}
.run-all-btn:hover:not(:disabled) { background: var(--accent); color: #0d1b2a; }
.run-all-btn:disabled { opacity: 0.5; cursor: not-allowed; }
.editor-hint { font-size: 10px; color: var(--text-muted); }
.toolbar-spacer { flex: 1; }

/* Monaco */
.monaco-container { flex: 1; overflow: hidden; }
</style>
