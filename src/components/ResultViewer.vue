<script setup lang="ts">
import { ref, computed, watch, reactive, nextTick, inject, onMounted, onBeforeUnmount } from 'vue'
import * as monaco from 'monaco-editor'
import { useEditorStore } from '../stores/editor'
import { useConnectionsStore } from '../stores/connections'
import { useSettingsStore } from '../stores/settings'
import { executeQuery } from '../lib/tauri'

const props = defineProps<{ tabIndex: number }>()

// ── Register BSON language for Monaco (once) ─────────────────────────────────
if (!monaco.languages.getLanguages().some(l => l.id === 'bson')) {
  monaco.languages.register({ id: 'bson' })
  monaco.languages.setMonarchTokensProvider('bson', {
    tokenizer: {
      root: [
        // BSON type wrappers — matched before strings
        [/(ObjectId)\(/, ['type.identifier', '@bsonArg']],
        [/(ISODate)\(/, ['type.identifier', '@bsonArg']],
        [/(Date)\(/, ['type.identifier', '@bsonArg']],
        [/(NumberDecimal)\(/, ['type.identifier', '@bsonArg']],
        [/(NumberLong)\(/, ['type.identifier', '@bsonArg']],
        [/(UUID)\(/, ['type.identifier', '@bsonArg']],
        [/(Binary)\(/, ['type.identifier', '@bsonArg']],
        // Standard JSON tokens
        [/"(?:[^"\\]|\\.)*"(?=\s*:)/, 'string.key'],   // keys
        [/"(?:[^"\\]|\\.)*"/, 'string.value'],          // string values
        [/-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/, 'number'],
        [/\btrue\b|\bfalse\b/, 'keyword'],
        [/\bnull\b/, 'keyword'],
        [/[{}[\],:]/, 'delimiter'],
      ],
      bsonArg: [
        [/"(?:[^"\\]|\\.)*"/, 'string.bson'],
        [/-?\d+(?:\.\d+)?/, 'number.bson'],
        [/\)/, 'type.identifier', '@pop'],
      ],
    },
  } as monaco.languages.IMonarchLanguage)

  // Define BSON theme rules for dark and light
  monaco.editor.defineTheme('bson-dark', {
    base: 'vs-dark',
    inherit: true,
    rules: [
      { token: 'type.identifier', foreground: 'c678dd', fontStyle: 'bold' },
      { token: 'string.bson', foreground: '56d4dd' },
      { token: 'number.bson', foreground: 'd19a66' },
      { token: 'string.key', foreground: 'e06c75' },
      { token: 'string.value', foreground: '98c379' },
      { token: 'number', foreground: 'd19a66' },
      { token: 'keyword', foreground: '56b6c2' },
      { token: 'delimiter', foreground: 'abb2bf' },
    ],
    colors: {},
  })
  monaco.editor.defineTheme('bson-light', {
    base: 'vs',
    inherit: true,
    rules: [
      { token: 'type.identifier', foreground: '7c3aed', fontStyle: 'bold' },
      { token: 'string.bson', foreground: '0891b2' },
      { token: 'number.bson', foreground: 'b45309' },
      { token: 'string.key', foreground: 'be185d' },
      { token: 'string.value', foreground: '16a34a' },
      { token: 'number', foreground: 'b45309' },
      { token: 'keyword', foreground: '0284c7' },
      { token: 'delimiter', foreground: '374151' },
    ],
    colors: {},
  })
}

const editorStore = useEditorStore()
const connStore = useConnectionsStore()
const settingsStore = useSettingsStore()
const toast = inject<(msg: string, type?: 'success' | 'error' | 'info') => void>('toast')!

// ── BSON display helpers (Studio 3T-like output) ─────────────────────────────
const BSON_WRAPPER_RE = /^(ObjectId|ISODate|Date|NumberDecimal|NumberLong|UUID|Binary)\(.*\)$/

/** Post-process JSON.stringify output: unquote BSON wrapper strings for display */
function bsonifyJson(json: string): string {
  return json.replace(/"(ObjectId|ISODate|NumberDecimal|NumberLong|UUID|Binary)\(\\"(.*?)\\"\)"/g,
    (_, type: string, inner: string) => `${type}("${inner}")`)
}

/** Convert BSON notation back to valid JSON for JSON.parse */
function jsonifyBson(text: string): string {
  return text.replace(/(ObjectId|ISODate|Date|NumberDecimal|NumberLong|UUID|Binary)\("([^"]*?)"\)/g,
    (_, type: string, inner: string) => `"${type}(\\"${inner}\\")"`)
}

/** Serialize a value for building MongoDB shell queries — BSON wrappers stay unquoted */
function bsonSerialize(value: unknown): string {
  if (typeof value === 'string' && BSON_WRAPPER_RE.test(value)) {
    return value
  }
  if (typeof value === 'object' && value !== null) {
    if (Array.isArray(value)) {
      return `[${value.map(bsonSerialize).join(', ')}]`
    }
    const entries = Object.entries(value as Record<string, unknown>)
      .map(([k, v]) => `"${k}": ${bsonSerialize(v)}`)
      .join(', ')
    return `{ ${entries} }`
  }
  return JSON.stringify(value)
}

function isBsonWrapper(v: unknown): boolean {
  return typeof v === 'string' && BSON_WRAPPER_RE.test(v)
}
const activeViewTab = ref<'table' | 'json' | 'tree'>(settingsStore.resultView)

function setActiveTab(tab: 'table' | 'json' | 'tree') {
  activeViewTab.value = tab
  settingsStore.setResultView(tab)
}

// ── Tab-specific state (reads from the tab at props.tabIndex, not activeTab) ──
// NOTE: We access editorStore.tabs[props.tabIndex] directly in each computed
// instead of using an intermediate `tab` computed. This is critical because
// `tabs` is a shallowRef — the tab objects inside keep the same identity after
// triggerTabsUpdate(). An intermediate computed would return the same object
// reference, and Vue's Object.is optimization would block dependent re-evaluation.

const results = computed(() => editorStore.tabs[props.tabIndex]?.results ?? [])

const result = computed(() => {
  const t = editorStore.tabs[props.tabIndex]
  if (!t?.results.length) return null
  return t.results[t.activeResultIdx]?.result ?? null
})

const rows = computed((): Record<string, unknown>[] => (result.value?.data ?? []) as Record<string, unknown>[])

// Tab connection context (for pagination, editing)
const tabConn = computed(() => {
  const t = editorStore.tabs[props.tabIndex]
  if (t?.connId) {
    return connStore.connections.find((c: { id: string }) => c.id === t.connId) ?? connStore.activeConn
  }
  return connStore.activeConn
})
const tabDb = computed(() => editorStore.tabs[props.tabIndex]?.dbName || connStore.activeDb || '')
const tabCollection = computed(() => editorStore.tabs[props.tabIndex]?.collectionName || connStore.activeCollection || '')

// Per-tab computed helpers for pagination state
const stmtActiveIdx = computed({
  get: () => editorStore.tabs[props.tabIndex]?.activeResultIdx ?? 0,
  set: (v: number) => {
    const t = editorStore.tabs[props.tabIndex]
    if (t) { t.activeResultIdx = v; editorStore.triggerTabsUpdate() }
  },
})

const queryLimit = computed({
  get: () => editorStore.tabs[props.tabIndex]?.queryLimit ?? 50,
  set: (v: number) => {
    const t = editorStore.tabs[props.tabIndex]
    if (t) { t.queryLimit = v; editorStore.triggerTabsUpdate() }
  },
})

const currentPage = computed(() => editorStore.tabs[props.tabIndex]?.currentPage ?? 0)
const canPaginate = computed(() => !!(editorStore.tabs[props.tabIndex]?.rawStmt))

// ── Result search/filter ──────────────────────────────────────────────────────
const resultSearch = ref('')
const showResultSearch = ref(false)
const resultSearchInput = ref<HTMLInputElement | null>(null)

function flattenValue(v: unknown): string {
  if (v === null || v === undefined) return 'null'
  if (typeof v === 'object') return JSON.stringify(v).toLowerCase()
  return String(v).toLowerCase()
}

const filteredRows = computed((): Record<string, unknown>[] => {
  const term = resultSearch.value.trim().toLowerCase()
  if (!term) return rows.value
  return rows.value.filter(row =>
    Object.entries(row).some(([k, v]) =>
      k.toLowerCase().includes(term) || flattenValue(v).includes(term)
    )
  )
})

function toggleResultSearch() {
  showResultSearch.value = !showResultSearch.value
  if (showResultSearch.value) {
    nextTick(() => resultSearchInput.value?.focus())
  } else {
    resultSearch.value = ''
  }
}

function clearResultSearch() {
  resultSearch.value = ''
  resultSearchInput.value?.focus()
}

const columns = computed(() => {
  if (!filteredRows.value.length) return []
  const keys = new Set<string>()
  filteredRows.value.slice(0, 50).forEach(row => Object.keys(row).forEach(k => keys.add(k)))
  return Array.from(keys)
})

const jsonText = computed(() =>
  result.value?.data ? bsonifyJson(JSON.stringify(result.value.data, null, 2)) : ''
)

function exportJSON() {
  download(new Blob([jsonText.value], { type: 'application/json' }), 'export.json')
}

function exportCSV() {
  if (!rows.value.length) return
  const header = columns.value.join(',')
  const lines = rows.value.map(row =>
    columns.value.map(c => {
      const v = row[c]
      const str = v === null || v === undefined ? '' : typeof v === 'object' ? JSON.stringify(v) : String(v)
      return `"${str.replace(/"/g, '""')}"`
    }).join(',')
  )
  download(new Blob([header + '\n' + lines.join('\n')], { type: 'text/csv' }), 'export.csv')
}

function download(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url; a.download = filename; a.click()
  URL.revokeObjectURL(url)
}

const expandedPaths = ref<Set<string>>(new Set())
function togglePath(path: string) {
  const s = new Set(expandedPaths.value)
  s.has(path) ? s.delete(path) : s.add(path)
  expandedPaths.value = s
}
function isExpanded(path: string) { return expandedPaths.value.has(path) }

const jsonExpandedPaths = ref<Set<string>>(new Set())
function toggleJsonPath(path: string) {
  const s = new Set(jsonExpandedPaths.value)
  s.has(path) ? s.delete(path) : s.add(path)
  jsonExpandedPaths.value = s
}
// Auto-expand top-level documents when result changes (new query or switching statement tabs)
watch(result, () => {
  const paths = new Set<string>()
  rows.value.forEach((_, i) => paths.add(`j${i}`))
  jsonExpandedPaths.value = paths
  resultSearch.value = ''
})

const copiedKey = ref<string | null>(null)
function copyCell(rowIdx: number, col: string, value: unknown) {
  const str = value === null || value === undefined ? '' : isBsonWrapper(value) ? String(value) : typeof value === 'object' ? JSON.stringify(value) : String(value)
  navigator.clipboard.writeText(str)
  const key = `${rowIdx}:${col}`
  copiedKey.value = key
  toast(`Copied: ${str.length > 40 ? str.slice(0, 40) + '…' : str}`, 'info')
  setTimeout(() => { if (copiedKey.value === key) copiedKey.value = null }, 800)
}

// ── Document side panel ───────────────────────────────────────────────────────
const selectedDoc = ref<Record<string, unknown> | null>(null)
const selectedDocIdx = ref(0)
const panelPaths = ref<Set<string>>(new Set())

function selectRow(row: Record<string, unknown>, idx: number) {
  selectedDoc.value = row
  selectedDocIdx.value = idx
  // Auto-expand root + any top-level object/array values
  const paths = new Set(['panel'])
  Object.entries(row).forEach(([k, v]) => {
    if (v !== null && typeof v === 'object') paths.add(`panel.${k}`)
  })
  panelPaths.value = paths
}

function togglePanelPath(path: string) {
  const s = new Set(panelPaths.value)
  s.has(path) ? s.delete(path) : s.add(path)
  panelPaths.value = s
}

async function doPaginate(direction: number) {
  await editorStore.paginateTab(props.tabIndex, currentPage.value + direction)
}

function onKeyDown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    if (editDocModal.open) { closeEditDocModal(); return }
    if (showResultSearch.value) { showResultSearch.value = false; resultSearch.value = ''; return }
    selectedDoc.value = null
  }
  // Cmd+Enter to save is handled by Monaco inside the modal
  if (!editDocModal.open && e.key === 'f' && (e.metaKey || e.ctrlKey)) {
    e.preventDefault()
    toggleResultSearch()
  }
}
onMounted(() => document.addEventListener('keydown', onKeyDown))
onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKeyDown)
  if (editDocEditor) { editDocEditor.dispose(); editDocEditor = null }
})

// ── Full document edit modal ──────────────────────────────────────────────────
const editDocModal = reactive<{ open: boolean; row: Record<string, unknown>; idx: number }>({ open: false, row: {}, idx: 0 })
const editDocJson = ref('')
const editDocError = ref('')
const editDocSaving = ref(false)
const editDocMonacoContainer = ref<HTMLDivElement | null>(null)
let editDocEditor: monaco.editor.IStandaloneCodeEditor | null = null

function openEditDocModal(row: Record<string, unknown>, idx: number, e: MouseEvent) {
  e.stopPropagation()
  editDocModal.open = true
  editDocModal.row = row
  editDocModal.idx = idx
  editDocJson.value = bsonifyJson(JSON.stringify(row, null, 2))
  editDocError.value = ''

  // Create Monaco instance after DOM renders
  nextTick(() => {
    if (!editDocMonacoContainer.value) return
    const theme = settingsStore.theme === 'light' ? 'bson-light' : 'bson-dark'
    editDocEditor = monaco.editor.create(editDocMonacoContainer.value, {
      value: editDocJson.value,
      language: 'bson',
      theme,
      fontSize: settingsStore.fontSize,
      fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
      fontLigatures: true,
      lineNumbers: 'on',
      minimap: { enabled: false },
      scrollBeyondLastLine: false,
      wordWrap: 'on',
      tabSize: 2,
      automaticLayout: true,
      scrollbar: { vertical: 'auto', horizontal: 'auto', verticalScrollbarSize: 6, horizontalScrollbarSize: 6 },
      renderLineHighlight: 'line',
      formatOnPaste: true,
    })
    // Cmd+Enter → save (addAction disposes cleanly with the editor, preventing keybinding leaks)
    editDocEditor.addAction({ id: 'save-doc', label: 'Save Document', keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter], run: () => saveEditDoc() })
    // Escape → close modal
    editDocEditor.addAction({ id: 'close-edit-modal', label: 'Close Edit Modal', keybindings: [monaco.KeyCode.Escape], run: () => closeEditDocModal() })
    // Keep editDocJson in sync for saveEditDoc
    editDocEditor.onDidChangeModelContent(() => {
      editDocJson.value = editDocEditor!.getValue()
    })
    editDocEditor.focus()
  })
}

function closeEditDocModal() {
  editDocModal.open = false
  if (editDocEditor) {
    editDocEditor.dispose()
    editDocEditor = null
  }
}

// Update Monaco theme when app theme changes
watch(() => settingsStore.theme, (t) => {
  if (editDocEditor) {
    monaco.editor.setTheme(t === 'light' ? 'bson-light' : 'bson-dark')
  }
})

async function saveEditDoc() {
  if (!editDocModal.open) return // guard: leaked keybinding from disposed Monaco
  const conn = tabConn.value
  const db = tabDb.value
  const col = tabCollection.value
  if (!conn || !db || !col) { editDocError.value = 'No active collection selected'; return }

  let parsed: Record<string, unknown>
  try {
    // Convert BSON notation (ISODate(...), ObjectId(...)) back to JSON strings before parsing
    parsed = JSON.parse(jsonifyBson(editDocJson.value))
  } catch (e) {
    editDocError.value = `Invalid JSON: ${String(e)}`
    return
  }

  const row = editDocModal.row
  const idVal = row['_id']
  const filterStr = typeof idVal === 'string' && idVal.startsWith('ObjectId(')
    ? `{ _id: ${idVal} }`
    : `{ _id: ${JSON.stringify(idVal)} }`

  // $set all non-_id fields, $unset removed fields
  const { _id: _ignored, ...fields } = parsed
  const setFields = Object.entries(fields).map(([k, v]) => `"${k}": ${bsonSerialize(v)}`).join(', ')
  const removedKeys = Object.keys(row).filter(k => k !== '_id' && !(k in fields))

  let query: string
  if (removedKeys.length && setFields) {
    const unsetFields = removedKeys.map(k => `"${k}": ""`).join(', ')
    query = `db.getCollection("${col}").updateOne(${filterStr}, { $set: { ${setFields} }, $unset: { ${unsetFields} } })`
  } else if (removedKeys.length) {
    const unsetFields = removedKeys.map(k => `"${k}": ""`).join(', ')
    query = `db.getCollection("${col}").updateOne(${filterStr}, { $unset: { ${unsetFields} } })`
  } else {
    query = `db.getCollection("${col}").updateOne(${filterStr}, { $set: { ${setFields} } })`
  }

  editDocSaving.value = true
  editDocError.value = ''
  try {
    const res = await executeQuery(conn, db, query)
    if (!editDocModal.open) return // modal was closed while the save was in-flight
    if (res.success) {
      toast('Document saved', 'success')
      Object.assign(row, fields)
      for (const k of removedKeys) delete row[k]
      closeEditDocModal()
    } else {
      editDocError.value = res.error ?? 'Update failed'
    }
  } catch (e) {
    editDocError.value = String(e)
  } finally {
    editDocSaving.value = false
  }
}

// ── Inline document edit ──────────────────────────────────────────────────────
const editingRowIdx = ref<number | null>(null)
const editedCells = ref<Record<string, string>>({})
const savingRow = ref(false)

function startEdit(rowIdx: number, col: string, value: unknown) {
  if (col === '_id') return // never edit _id
  editingRowIdx.value = rowIdx
  editedCells.value = {}
  const str = value === null || value === undefined ? '' : isBsonWrapper(value) ? String(value) : typeof value === 'object' ? JSON.stringify(value) : String(value)
  editedCells.value[col] = str
}

function onCellInput(col: string, e: Event) {
  editedCells.value[col] = (e.target as HTMLInputElement).value
}

function cancelEdit() {
  editingRowIdx.value = null
  editedCells.value = {}
}

function parseEditedValue(str: string): string {
  const s = str.trim()
  if (s === 'null') return 'null'
  if (s === 'true') return 'true'
  if (s === 'false') return 'false'
  if (s !== '' && !isNaN(Number(s))) return s
  // BSON wrappers — output unquoted so Rust parser handles them
  if (BSON_WRAPPER_RE.test(s)) return s
  // string — wrap in quotes, escape internals
  return JSON.stringify(s)
}

async function saveRow(rowIdx: number) {
  const conn = tabConn.value
  const db = tabDb.value
  const col = tabCollection.value
  if (!conn || !db || !col) { toast('No active collection selected', 'error'); return }
  const row = rows.value[rowIdx]
  if (!row) return

  const dirty = Object.entries(editedCells.value)
  if (!dirty.length) { cancelEdit(); return }

  // Build filter from _id
  const idVal = row['_id']
  let filterStr = ''
  if (typeof idVal === 'string' && idVal.startsWith('ObjectId(')) {
    filterStr = `{ _id: ${idVal} }`
  } else {
    filterStr = `{ _id: ${JSON.stringify(idVal)} }`
  }

  // Build $set object
  const setFields = dirty.map(([k, v]) => `"${k}": ${parseEditedValue(v)}`).join(', ')
  const query = `db.getCollection("${col}").updateOne(${filterStr}, { $set: { ${setFields} } })`

  savingRow.value = true
  try {
    const result = await executeQuery(conn, db, query)
    if (result.success) {
      toast(`Saved: ${result.data?.[0] ? JSON.stringify(result.data[0]) : 'ok'}`, 'success')
      // Update local row data
      for (const [k, v] of dirty) {
        try { (row as Record<string, unknown>)[k] = JSON.parse(parseEditedValue(v)) } catch { (row as Record<string, unknown>)[k] = v }
      }
      cancelEdit()
    } else {
      toast(result.error ?? 'Update failed', 'error')
    }
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    savingRow.value = false
  }
}

function valueClass(v: unknown) {
  if (v === null) return 'val-null'
  if (typeof v === 'boolean') return 'val-bool'
  if (typeof v === 'number') return 'val-num'
  if (isBsonWrapper(v)) return 'val-bson'
  if (typeof v === 'string') return 'val-str'
  return ''
}
function displayValue(v: unknown): string {
  if (v === null) return 'null'
  if (isBsonWrapper(v)) return String(v)
  if (typeof v === 'string') return `"${v}"`
  return String(v)
}
</script>

<template>
  <div class="viewer-root">
    <div class="viewer-bar">
      <div class="view-tabs">
        <button :class="['view-tab', { active: activeViewTab === 'table' }]" @click="setActiveTab('table')">Table</button>
        <button :class="['view-tab', { active: activeViewTab === 'json' }]" @click="setActiveTab('json')">JSON</button>
        <button :class="['view-tab', { active: activeViewTab === 'tree' }]" @click="setActiveTab('tree')">Tree</button>
      </div>
      <span class="viewer-spacer" />
      <template v-if="result?.success && rows.length">
        <span class="row-count">
          {{ filteredRows.length !== rows.length ? `${filteredRows.length}/` : '' }}{{ rows.length }} rows
        </span>
        <button class="btn-ghost" style="font-size:11px" @click="exportCSV">↓ CSV</button>
        <button class="btn-ghost" style="font-size:11px" @click="exportJSON">↓ JSON</button>
        <button :class="['btn-ghost', 'search-toggle-btn', { active: showResultSearch }]" title="Search results (⌘F)" @click="toggleResultSearch">⌕</button>
      </template>
      <template v-if="canPaginate && result?.success">
        <div class="page-nav">
          <button class="page-btn" :disabled="currentPage === 0 || editorStore.isExecuting" @click="doPaginate(-1)">‹</button>
          <span class="page-label">p.{{ currentPage + 1 }}</span>
          <button class="page-btn" :disabled="rows.length < queryLimit || editorStore.isExecuting" @click="doPaginate(1)">›</button>
        </div>
      </template>
      <span class="limit-label">Limit</span>
      <select v-model.number="queryLimit" class="limit-select" title="Max documents to return">
        <option :value="50">50</option>
        <option :value="100">100</option>
        <option :value="200">200</option>
        <option :value="500">500</option>
      </select>
    </div>

    <!-- Result search bar -->
    <Transition name="search-bar">
      <div v-if="showResultSearch" class="result-search-bar">
        <span class="result-search-icon">⌕</span>
        <input
          ref="resultSearchInput"
          v-model="resultSearch"
          class="result-search-input"
          placeholder="Filter by key or value…"
          @keydown.escape="toggleResultSearch"
        />
        <span v-if="resultSearch" class="result-search-count">{{ filteredRows.length }} / {{ rows.length }}</span>
        <button v-if="resultSearch" class="result-search-clear" title="Clear" @click="clearResultSearch">✕</button>
      </div>
    </Transition>

    <!-- Statement tabs (only when multiple statements were run) -->
    <div v-if="results.length > 1" class="stmt-tabs">
      <button
        v-for="(item, i) in results"
        :key="i"
        :class="['stmt-tab', { active: stmtActiveIdx === i }]"
        @click="stmtActiveIdx = i"
      >
        {{ item.label }}
        <span :class="['stmt-badge', item.result.success ? 'ok' : 'err']">
          {{ item.result.success ? `${item.result.rows}r · ${item.result.elapsed_ms}ms` : '!' }}
        </span>
      </button>
    </div>

    <div class="viewer-body">
      <div v-if="!result" class="viewer-empty">Run a query to see results</div>

      <div v-else-if="!result.success" class="viewer-error">
        <div class="error-label">Error</div>
        <pre class="error-text">{{ result.error }}</pre>
      </div>

      <!-- Table -->
      <div v-else-if="activeViewTab === 'table'" class="viewer-content">
        <div v-if="!filteredRows.length" class="viewer-empty">{{ resultSearch ? 'No matches' : 'No documents returned' }}</div>
        <div v-else class="table-wrap">
          <table class="result-table">
            <thead>
              <tr>
                <th class="row-num">#</th>
                <th v-for="col in columns" :key="col">{{ col }}</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="(row, i) in filteredRows"
                :key="i"
                :class="{ 'row-selected': selectedDocIdx === i && selectedDoc !== null, 'row-editing': editingRowIdx === i }"
                @click="editingRowIdx === null ? selectRow(row, i) : undefined"
              >
                <td class="row-num row-num-actions" @click.stop="selectRow(row, i)">
                  <template v-if="editingRowIdx === i">
                    <button class="edit-save-btn" :disabled="savingRow" title="Save changes" @click.stop="saveRow(i)">{{ savingRow ? '…' : '✓' }}</button>
                    <button class="edit-cancel-btn" title="Cancel" @click.stop="cancelEdit">✕</button>
                  </template>
                  <template v-else>
                    <span class="row-num-label">{{ i + 1 }}</span>
                    <button class="row-edit-btn" title="Edit document" @click.stop="openEditDocModal(row, i, $event)">✎</button>
                  </template>
                </td>
                <td
                  v-for="col in columns"
                  :key="col"
                  :class="['cell', { copied: copiedKey === `${i}:${col}`, 'cell-editing': editingRowIdx === i && col !== '_id' }]"
                  :title="editingRowIdx === i ? (col === '_id' ? '_id (read-only)' : 'Double-click to edit') : 'Click to copy'"
                  @click.stop="editingRowIdx === null ? copyCell(i, col, row[col]) : undefined"
                  @dblclick.stop="startEdit(i, col, row[col])"
                >
                  <template v-if="editingRowIdx === i && col !== '_id' && col in editedCells">
                    <input
                      class="cell-input"
                      :value="editedCells[col]"
                      @input="onCellInput(col, $event)"
                      @keyup.enter="saveRow(i)"
                      @keyup.escape="cancelEdit"
                      @click.stop
                    />
                  </template>
                  <template v-else>
                    <span :class="valueClass(row[col])">
                      {{ row[col] === undefined ? '' : displayValue(row[col]) }}
                    </span>
                  </template>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>

      <!-- JSON (collapsible) -->
      <div v-else-if="activeViewTab === 'json'" class="viewer-content">
        <div v-if="!filteredRows.length" class="viewer-empty">{{ resultSearch ? 'No matches' : 'No documents returned' }}</div>
        <div v-else class="json-tree">
          <JsonNode
            v-for="(row, i) in filteredRows"
            :key="i"
            :path="`j${i}`"
            :field="(null as unknown as string)"
            :value="(row as unknown as null)"
            :depth="0"
            :expanded-paths="jsonExpandedPaths"
            :is-last="true"
            @toggle="toggleJsonPath"
          />
        </div>
      </div>

      <!-- Tree -->
      <div v-else-if="activeViewTab === 'tree'" class="viewer-content">
        <div v-if="!filteredRows.length" class="viewer-empty">{{ resultSearch ? 'No matches' : 'No documents returned' }}</div>
        <div v-else class="tree-view">
          <div v-for="(row, i) in filteredRows" :key="i" class="tree-doc">
            <div class="tree-doc-header" @click="togglePath(`doc_${i}`)">
              <span class="tree-caret">{{ isExpanded(`doc_${i}`) ? '▾' : '▸' }}</span>
              <span class="tree-key">Document {{ i + 1 }}</span>
            </div>
            <template v-if="isExpanded(`doc_${i}`)">
              <TreeNode
                v-for="(val, key) in row"
                :key="key"
                :path="`doc_${i}.${String(key)}`"
                :field="String(key)"
                :value="(val as null)"
                :depth="1"
                :expanded-paths="expandedPaths"
                @toggle="togglePath"
              />
            </template>
          </div>
        </div>
      </div>

      <!-- Full document edit modal -->
      <Teleport to="body">
        <Transition name="modal-fade">
          <div v-if="editDocModal.open" class="edit-doc-backdrop" @click.self="closeEditDocModal">
            <div class="edit-doc-modal">
              <div class="edit-doc-header">
                <span class="edit-doc-title">Edit Document {{ editDocModal.idx + 1 }}</span>
                <span class="edit-doc-hint-inline">⌘F to search</span>
                <button class="panel-close" @click="closeEditDocModal">✕</button>
              </div>
              <div class="edit-doc-body">
                <div ref="editDocMonacoContainer" class="edit-doc-monaco" />
              </div>
              <div class="edit-doc-footer">
                <span v-if="editDocError" class="edit-doc-error">{{ editDocError }}</span>
                <span v-else class="edit-doc-hint">⌘Enter to save &nbsp;·&nbsp; Esc to cancel</span>
                <div class="edit-doc-actions">
                  <button class="btn-ghost" @click="closeEditDocModal">Cancel</button>
                  <button class="btn-accent" :disabled="editDocSaving" @click="saveEditDoc">
                    {{ editDocSaving ? 'Saving…' : 'Save' }}
                  </button>
                </div>
              </div>
            </div>
          </div>
        </Transition>
      </Teleport>

      <!-- Document side panel -->
      <Transition name="panel">
        <div v-if="selectedDoc" class="doc-panel">
          <div class="panel-header">
            <span class="panel-title">Document {{ selectedDocIdx + 1 }}</span>
            <button class="panel-close" title="Close (Esc)" @click="selectedDoc = null">✕</button>
          </div>
          <div class="panel-body">
            <JsonNode
              path="panel"
              :field="(null as unknown as string)"
              :value="(selectedDoc as unknown as null)"
              :depth="0"
              :expanded-paths="panelPaths"
              :is-last="true"
              @toggle="togglePanelPath"
            />
          </div>
        </div>
      </Transition>
    </div>
  </div>
</template>

<!-- Sub-components: TreeNode + JsonNode -->
<script lang="ts">
import { defineComponent, h, type PropType, type VNode } from 'vue'

export const JsonNode = defineComponent({
  name: 'JsonNode',
  props: {
    path:          { type: String,                          required: true },
    field:         { type: Object as PropType<string | null>, default: null },
    value:         { type: Object as PropType<unknown>,     default: null },
    depth:         { type: Number,                          default: 0 },
    expandedPaths: { type: Object as PropType<Set<string>>, required: true },
    isLast:        { type: Boolean,                         default: true },
  },
  emits: ['toggle'],
  setup(props, { emit }): () => VNode {
    const isObj = (v: unknown): v is Record<string, unknown> =>
      v !== null && typeof v === 'object' && !Array.isArray(v)
    const isArr = (v: unknown): v is unknown[] => Array.isArray(v)

    return (): VNode => {
      const indent = props.depth * 16
      const expanded = props.expandedPaths.has(props.path)
      const suffix = props.isLast ? '' : ','

      const fieldPart = props.field !== null
        ? [h('span', { class: 'jk' }, `"${props.field}"`), h('span', { class: 'jp' }, ': ')]
        : []

      if (isObj(props.value) || isArr(props.value)) {
        const asArr = isArr(props.value)
        const entries: [string, unknown][] = asArr
          ? (props.value as unknown[]).map((v, i) => [String(i), v])
          : Object.entries(props.value as Record<string, unknown>)
        const count = entries.length
        const [open, close] = asArr ? ['[', ']'] : ['{', '}']

        if (count === 0) {
          return h('div', { class: 'jrow', style: { paddingLeft: `${indent}px` } }, [
            ...fieldPart,
            h('span', { class: 'jp' }, `${open}${close}${suffix}`),
          ])
        }

        return h('div', [
          h('div', {
            class: 'jrow jexpandable',
            style: { paddingLeft: `${indent}px` },
            onClick: () => emit('toggle', props.path),
          }, [
            h('span', { class: 'jcaret' }, expanded ? '▾' : '▸'),
            ...fieldPart,
            h('span', { class: 'jp' }, open + (expanded ? '' : ` … ${count} ${close}${suffix}`)),
          ]),
          ...(expanded
            ? [
                ...entries.map(([k, v], idx) =>
                  h(JsonNode, {
                    key: `${props.path}.${k}`,
                    path: `${props.path}.${k}`,
                    field: k,
                    value: v as null,
                    depth: props.depth + 1,
                    expandedPaths: props.expandedPaths,
                    isLast: idx === entries.length - 1,
                    onToggle: (p: string) => emit('toggle', p),
                  })
                ),
                h('div', { class: 'jrow', style: { paddingLeft: `${indent}px` } },
                  h('span', { class: 'jp' }, `${close}${suffix}`)
                ),
              ]
            : []
          ),
        ])
      }

      // Primitive
      const bsonPattern = /^(ObjectId|ISODate|Binary)\("(.*)"\)$/
      const cls = (v: unknown) => {
        if (v === null)             return 'jv-null'
        if (typeof v === 'boolean') return 'jv-bool'
        if (typeof v === 'number')  return 'jv-num'
        if (typeof v === 'string') {
          const m = bsonPattern.exec(v)
          if (m) {
            if (m[1] === 'ObjectId') return 'jv-oid'
            if (m[1] === 'ISODate')  return 'jv-date'
            return 'jv-bin'
          }
          return 'jv-str'
        }
        return ''
      }
      const disp = (v: unknown): string => {
        if (v === null) return 'null'
        if (typeof v === 'string') {
          if (bsonPattern.test(v)) return v  // ObjectId("..."), ISODate("...") shown as-is
          return `"${v}"`
        }
        return String(v)
      }

      return h('div', { class: 'jrow', style: { paddingLeft: `${indent}px` } }, [
        ...fieldPart,
        h('span', { class: cls(props.value) }, disp(props.value) + suffix),
      ])
    }
  },
})

export const TreeNode = defineComponent({
  name: 'TreeNode',
  props: {
    path:          { type: String,                          required: true },
    field:         { type: String,                          required: true },
    value:         { type: Object as PropType<unknown>,     default: null },
    depth:         { type: Number,                          default: 0 },
    expandedPaths: { type: Object as PropType<Set<string>>, required: true },
  },
  emits: ['toggle'],
  setup(props, { emit }): () => VNode {
    const isObj = (v: unknown): v is Record<string, unknown> =>
      v !== null && typeof v === 'object' && !Array.isArray(v)
    const isArr = (v: unknown): v is unknown[] => Array.isArray(v)
    const bsonRe = /^(ObjectId|ISODate|Date|NumberDecimal|NumberLong|UUID|Binary)\(.*\)$/
    const cls = (v: unknown) => {
      if (v === null)            return 'val-null'
      if (typeof v === 'boolean') return 'val-bool'
      if (typeof v === 'number')  return 'val-num'
      if (typeof v === 'string' && bsonRe.test(v)) return 'val-bson'
      if (typeof v === 'string')  return 'val-str'
      return ''
    }
    const disp = (v: unknown): string => {
      if (v === null) return 'null'
      if (typeof v === 'string' && bsonRe.test(v)) return v
      if (typeof v === 'string') return `"${v}"`
      return String(v)
    }

    return (): VNode => {
      const indent = props.depth * 14
      const expanded = props.expandedPaths.has(props.path)

      if (isObj(props.value) || isArr(props.value)) {
        const asArr = isArr(props.value)
        const entries: [string, unknown][] = asArr
          ? (props.value as unknown[]).map((v, i) => [String(i), v])
          : Object.entries(props.value as Record<string, unknown>)
        const count = entries.length
        const [open, close] = asArr ? ['[', ']'] : ['{', '}']

        return h('div', [
          h('div', {
            class: 'tree-row tree-expandable',
            style: { paddingLeft: `${indent}px` },
            onClick: () => emit('toggle', props.path),
          }, [
            h('span', { class: 'tree-caret' }, expanded ? '▾' : '▸'),
            h('span', { class: 'tree-key' }, `${props.field}: `),
            h('span', { class: 'val-dim' }, open + (expanded ? '' : ` … ${count} ${close}`)),
          ]),
          ...(expanded
            ? entries.map(([k, v]) =>
                h(TreeNode, {
                  key: `${props.path}.${k}`,
                  path: `${props.path}.${k}`,
                  field: k,
                  value: (v as null),
                  depth: props.depth + 1,
                  expandedPaths: props.expandedPaths,
                  onToggle: (p: string) => emit('toggle', p),
                })
              )
            : []),
          expanded
            ? h('div', { style: { paddingLeft: `${indent}px`, color: 'var(--text-muted)', fontSize: '11px' } }, close)
            : null,
        ])
      }

      return h('div', {
        class: 'tree-row',
        style: { paddingLeft: `${indent}px` },
      }, [
        h('span', { class: 'tree-key' }, `${props.field}: `),
        h('span', { class: cls(props.value) }, disp(props.value)),
      ])
    }
  },
})
</script>

<style scoped>
.viewer-root { display: flex; flex-direction: column; height: 100%; overflow: hidden; background: var(--bg); }

.viewer-bar {
  display: flex; align-items: center; gap: 4px;
  padding: 4px 8px; background: var(--bg-card);
  border-bottom: 1px solid var(--border); flex-shrink: 0;
}
.view-tabs { display: flex; gap: 2px; }
.view-tab {
  font-size: 11px; padding: 2px 10px;
  background: transparent; color: var(--text-dim);
  border: 1px solid transparent; border-radius: var(--radius);
}
.view-tab:hover { background: var(--bg-hover); color: var(--text); }
.view-tab.active { background: var(--bg-active); color: var(--accent); border-color: var(--border); }
.viewer-spacer { flex: 1; }
.row-count { font-size: 10px; color: var(--text-muted); padding: 0 4px; }
.page-nav { display: flex; align-items: center; gap: 2px; margin: 0 2px; }
.page-btn {
  font-size: 13px; line-height: 1; padding: 1px 5px;
  background: transparent; color: var(--text-dim);
  border: 1px solid var(--border); border-radius: var(--radius);
}
.page-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text); }
.page-btn:disabled { opacity: 0.3; cursor: not-allowed; }
.page-label { font-size: 10px; color: var(--text-muted); padding: 0 2px; }
.limit-label { font-size: 10px; color: var(--text-muted); }
.limit-select {
  font-size: 11px; padding: 2px 2px;
  width: 56px;
  background: var(--bg); color: var(--text);
  border: 1px solid var(--border); border-radius: var(--radius);
  cursor: pointer;
}

/* Result search bar */
.search-toggle-btn { font-size: 14px; padding: 1px 5px; }
.search-toggle-btn.active { color: var(--accent); }
.result-search-bar {
  display: flex; align-items: center; gap: 6px;
  padding: 4px 10px; background: var(--bg-card);
  border-bottom: 1px solid var(--border); flex-shrink: 0;
}
.result-search-icon { font-size: 14px; color: var(--text-muted); }
.result-search-input {
  flex: 1; min-width: 0;
  background: var(--bg); color: var(--text);
  border: 1px solid var(--border); border-radius: var(--radius);
  padding: 3px 8px; font-size: 12px; outline: none;
}
.result-search-input:focus { border-color: var(--accent); }
.result-search-count { font-size: 10px; color: var(--text-muted); white-space: nowrap; }
.result-search-clear {
  font-size: 10px; padding: 1px 4px; border-radius: 2px;
  background: transparent; color: var(--text-muted);
}
.result-search-clear:hover { background: var(--bg-hover); color: var(--red); }
.search-bar-enter-active, .search-bar-leave-active { transition: all 0.12s ease; overflow: hidden; }
.search-bar-enter-from, .search-bar-leave-to { max-height: 0; opacity: 0; padding-top: 0; padding-bottom: 0; }
.search-bar-enter-to, .search-bar-leave-from { max-height: 40px; opacity: 1; }

/* Statement tabs */
.stmt-tabs {
  display: flex; gap: 2px; padding: 4px 8px;
  background: var(--bg-card); border-bottom: 1px solid var(--border);
  flex-shrink: 0; overflow-x: auto;
}
.stmt-tab {
  display: flex; align-items: center; gap: 5px;
  font-size: 11px; padding: 2px 8px;
  background: transparent; color: var(--text-dim);
  border: 1px solid transparent; border-radius: var(--radius);
  white-space: nowrap; cursor: pointer;
}
.stmt-tab:hover { background: var(--bg-hover); color: var(--text); }
.stmt-tab.active { background: var(--bg-active); color: var(--text); border-color: var(--border); }
.stmt-badge { font-size: 10px; padding: 1px 4px; border-radius: 8px; }
.stmt-badge.ok { background: color-mix(in srgb, var(--green) 15%, transparent); color: var(--green); }
.stmt-badge.err { background: color-mix(in srgb, var(--red) 15%, transparent); color: var(--red); }

.viewer-empty {
  flex: 1; display: flex; align-items: center; justify-content: center;
  color: var(--text-muted); font-size: 12px; font-style: italic;
}
.viewer-error { flex: 1; padding: 12px; overflow: auto; }
.error-label { color: var(--red); font-weight: 600; font-size: var(--font-size-ui, 13px); margin-bottom: 8px; }
.error-text { color: var(--red); font-family: var(--font-mono); font-size: var(--font-size-ui, 13px); white-space: pre-wrap; }

.viewer-body { flex: 1; position: relative; overflow: hidden; display: flex; flex-direction: column; }
.viewer-content { flex: 1; overflow: auto; }

/* Table */
.table-wrap { overflow: auto; height: 100%; }
.result-table { width: max-content; min-width: 100%; border-collapse: collapse; font-size: var(--font-size-ui, 13px); }
.result-table th {
  position: sticky; top: 0; background: var(--bg-card);
  padding: 5px 10px; text-align: left; white-space: nowrap;
  border-bottom: 1px solid var(--border); border-right: 1px solid var(--border);
  color: var(--text-dim); font-weight: 600; user-select: none;
}
.result-table td {
  padding: 3px 10px;
  border-bottom: 1px solid var(--border); border-right: 1px solid var(--border);
  max-width: 300px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.result-table tr { cursor: pointer; }
.result-table tr:hover td { background: var(--bg-hover); }
.result-table tr.row-selected td { background: color-mix(in srgb, var(--accent) 8%, transparent); }
.result-table td.cell { cursor: pointer; }
.result-table td.copied { background: color-mix(in srgb, var(--accent) 15%, transparent) !important; transition: background 0.1s; }
.result-table tr.row-editing td { background: color-mix(in srgb, var(--yellow) 5%, transparent); }
.result-table td.cell-editing { cursor: text; padding: 1px 4px; }
.cell-input {
  width: 100%; min-width: 80px; max-width: 280px;
  background: var(--bg); color: var(--text);
  border: 1px solid var(--accent); border-radius: 2px;
  padding: 1px 4px; font-size: var(--font-size-ui, 13px); font-family: var(--font-mono);
  outline: none;
}
.edit-save-btn, .edit-cancel-btn {
  font-size: 10px; padding: 0 3px; border-radius: 2px;
  background: transparent; line-height: 1.4;
}
.edit-save-btn { color: var(--green); }
.edit-save-btn:hover:not(:disabled) { background: color-mix(in srgb, var(--green) 15%, transparent); }
.edit-cancel-btn { color: var(--red); }
.edit-cancel-btn:hover { background: color-mix(in srgb, var(--red) 15%, transparent); }

/* Row edit pencil button */
.row-num-actions { position: relative; }
.row-num-label { display: inline-block; }
.row-edit-btn {
  display: none;
  font-size: 12px; padding: 0 3px; border-radius: 2px;
  background: transparent; color: var(--accent); opacity: 0.8;
  position: absolute; right: 2px; top: 50%; transform: translateY(-50%);
}
.row-edit-btn:hover { opacity: 1; background: color-mix(in srgb, var(--accent) 15%, transparent); }
.result-table tr:hover .row-edit-btn { display: inline-block; }
.result-table tr:hover .row-num-label { opacity: 0.4; }

/* Full document edit modal */
.edit-doc-backdrop {
  position: fixed; inset: 0; z-index: 1000;
  background: rgba(0,0,0,.55); backdrop-filter: blur(2px);
  display: flex; align-items: center; justify-content: center;
}
.edit-doc-modal {
  display: flex; flex-direction: column;
  width: min(720px, 92vw); height: min(600px, 85vh);
  background: var(--bg-card); border: 1px solid var(--border);
  border-radius: 8px; overflow: hidden;
  box-shadow: 0 24px 80px rgba(0,0,0,.4);
}
.edit-doc-header {
  display: flex; align-items: center; gap: 8px;
  padding: 8px 12px; border-bottom: 1px solid var(--border); flex-shrink: 0;
}
.edit-doc-title { font-size: 12px; font-weight: 600; color: var(--text-dim); flex-shrink: 0; }
.edit-doc-hint-inline { font-size: 10px; color: var(--text-muted); flex: 1; text-align: right; margin-right: 8px; }
.edit-doc-body { flex: 1; overflow: hidden; }
.edit-doc-monaco { width: 100%; height: 100%; }
.edit-doc-footer {
  display: flex; align-items: center; justify-content: space-between;
  padding: 8px 12px; border-top: 1px solid var(--border); flex-shrink: 0;
  gap: 8px;
}
.edit-doc-hint { font-size: 10px; color: var(--text-muted); }
.edit-doc-error { font-size: 11px; color: var(--red); flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.edit-doc-actions { display: flex; gap: 6px; flex-shrink: 0; }
.btn-accent {
  font-size: 12px; padding: 4px 14px;
  background: var(--accent); color: #fff;
  border-radius: var(--radius); border: none; cursor: pointer; font-weight: 600;
}
.btn-accent:hover:not(:disabled) { opacity: 0.88; }
.btn-accent:disabled { opacity: 0.4; cursor: not-allowed; }

/* Modal transition */
.modal-fade-enter-active, .modal-fade-leave-active { transition: opacity 0.15s ease; }
.modal-fade-enter-from, .modal-fade-leave-to { opacity: 0; }
.modal-fade-enter-active .edit-doc-modal, .modal-fade-leave-active .edit-doc-modal { transition: transform 0.15s ease; }
.modal-fade-enter-from .edit-doc-modal, .modal-fade-leave-to .edit-doc-modal { transform: scale(0.96); }

/* Document side panel */
.doc-panel {
  position: absolute; top: 0; right: 0; bottom: 0; width: 360px;
  background: var(--bg-card); border-left: 1px solid var(--border);
  display: flex; flex-direction: column; z-index: 10;
}
.panel-header {
  display: flex; align-items: center; justify-content: space-between;
  padding: 6px 10px; border-bottom: 1px solid var(--border); flex-shrink: 0;
}
.panel-title { font-size: 11px; font-weight: 600; color: var(--text-dim); }
.panel-close {
  font-size: 11px; background: transparent; color: var(--text-muted);
  border-radius: 2px; padding: 1px 4px;
}
.panel-close:hover { background: var(--bg-hover); color: var(--red); }
.panel-body { flex: 1; overflow: auto; padding: 8px 10px; font-family: var(--font-mono); font-size: var(--font-size-ui, 13px); }

/* Panel slide transition */
.panel-enter-active, .panel-leave-active { transition: transform 0.18s ease; }
.panel-enter-from, .panel-leave-to { transform: translateX(100%); }
.row-num { color: var(--text-muted); font-size: 11px; text-align: right; min-width: 36px; }

/* JSON tree — :deep() needed because JsonNode renders via h() without scoped attributes */
.json-tree { padding: 8px 12px; font-family: var(--font-mono); font-size: var(--font-size-ui, 13px); }
:deep(.jrow) { display: flex; align-items: baseline; gap: 2px; padding: 1px 0; line-height: 1.6; white-space: nowrap; }
:deep(.jexpandable) { cursor: pointer; border-radius: 2px; }
:deep(.jexpandable:hover) { background: var(--bg-hover); }
:deep(.jcaret) { font-size: 9px; color: var(--text-muted); width: 12px; flex-shrink: 0; }
:deep(.jk)  { color: var(--json-key, #58a6ff); }
:deep(.jp)  { color: var(--text-muted); }
:deep(.jv-str)  { color: var(--json-str, #3fb950); }
:deep(.jv-num)  { color: var(--json-num, #d29922); }
:deep(.jv-bool) { color: var(--json-bool, #f7941d); }
:deep(.jv-null) { color: var(--text-muted); font-style: italic; }
:deep(.jv-oid)  { color: var(--json-oid, #bc8cff); }
:deep(.jv-date) { color: var(--json-date, #56d4dd); }
:deep(.jv-bin)  { color: var(--json-bin, #f0883e); }

/* Tree — same :deep() treatment for TreeNode */
.tree-view { padding: 6px; }
:deep(.tree-doc) { margin-bottom: 4px; border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; }
:deep(.tree-doc-header) {
  display: flex; align-items: center; gap: 4px;
  padding: 4px 8px; background: var(--bg-card); cursor: pointer;
  font-size: 11px; font-weight: 600; color: var(--text-dim);
}
:deep(.tree-doc-header:hover) { background: var(--bg-hover); }
:deep(.tree-row) { display: flex; align-items: baseline; gap: 4px; padding: 1px 8px; font-size: var(--font-size-ui, 13px); font-family: var(--font-mono); }
:deep(.tree-row.tree-expandable) { cursor: pointer; }
:deep(.tree-row.tree-expandable:hover) { background: var(--bg-hover); }
:deep(.tree-caret) { font-size: 9px; color: var(--text-muted); width: 10px; flex-shrink: 0; }
:deep(.tree-key)  { color: var(--json-key, #58a6ff); }
:deep(.val-str)   { color: var(--json-str, #3fb950); }
:deep(.val-num)   { color: var(--json-num, #d29922); }
:deep(.val-bool)  { color: var(--json-bool, #f7941d); }
:deep(.val-null)  { color: var(--text-muted); font-style: italic; }
:deep(.val-bson)  { color: var(--json-bson, #c678dd); }
:deep(.val-dim)   { color: var(--text-muted); }
</style>
