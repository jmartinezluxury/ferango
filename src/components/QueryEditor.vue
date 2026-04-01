<script lang="ts">
// Module-level: shared across ALL QueryEditor instances
// Prevents duplicate global Monaco provider registrations
let _globalProvidersRegistered = false
let _globalCachedFields: string[] = []
let _globalFieldDisposable: import('monaco-editor').IDisposable | null = null
let _globalCollectionDisposable: import('monaco-editor').IDisposable | null = null
</script>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch, inject } from 'vue'
import * as monaco from 'monaco-editor'
import { useEditorStore } from '../stores/editor'
import type { StatementResult } from '../stores/editor'
import { useConnectionsStore } from '../stores/connections'
import { useSettingsStore } from '../stores/settings'
import { executeQuery, getFieldPaths, listCollections, logQuery, aiComplete } from '../lib/tauri'

const props = defineProps<{ tabIndex: number }>()

const editorStore = useEditorStore()
const connStore = useConnectionsStore()
const settingsStore = useSettingsStore()
const toast = inject<(msg: string, type?: 'success' | 'error' | 'info') => void>('toast')!

const containerEl = ref<HTMLDivElement | null>(null)
let monacoEditor: monaco.editor.IStandaloneCodeEditor | null = null
let autoSaveTimer: ReturnType<typeof setTimeout> | null = null

// ── Monaco setup ──────────────────────────────────────────────────────────────
onMounted(() => {
  if (!containerEl.value) return

  monacoEditor = monaco.editor.create(containerEl.value, {
    value: editorStore.tabs[props.tabIndex]?.content ?? '// Select a collection from the tree or open a script\n',
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

  // ── Global providers (registered only once across all instances) ──────────
  if (!_globalProvidersRegistered) {
    _globalProvidersRegistered = true

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

    // Register MongoDB operator completions (triggered by '$')
    monaco.languages.registerCompletionItemProvider('javascript', {
      triggerCharacters: ['$'],
      provideCompletionItems(model, position) {
        const lineContent = model.getLineContent(position.lineNumber)
        let startCol = position.column
        while (startCol > 1 && lineContent[startCol - 2] !== '$') startCol--
        if (startCol > 1 && lineContent[startCol - 2] === '$') startCol--
        const range = {
          startLineNumber: position.lineNumber, endLineNumber: position.lineNumber,
          startColumn: startCol, endColumn: position.column,
        }

        const operators: { label: string; doc: string; text: string; kind: monaco.languages.CompletionItemKind }[] = [
          // ── Comparison ──
          { label: '$eq',  doc: 'Matches values equal to a specified value', text: '\\$eq: ${1:value}', kind: monaco.languages.CompletionItemKind.Operator },
          { label: '$ne',  doc: 'Matches values not equal to a specified value', text: '\\$ne: ${1:value}', kind: monaco.languages.CompletionItemKind.Operator },
          { label: '$gt',  doc: 'Matches values greater than a specified value', text: '\\$gt: ${1:value}', kind: monaco.languages.CompletionItemKind.Operator },
          { label: '$gte', doc: 'Matches values greater than or equal to a specified value', text: '\\$gte: ${1:value}', kind: monaco.languages.CompletionItemKind.Operator },
          { label: '$lt',  doc: 'Matches values less than a specified value', text: '\\$lt: ${1:value}', kind: monaco.languages.CompletionItemKind.Operator },
          { label: '$lte', doc: 'Matches values less than or equal to a specified value', text: '\\$lte: ${1:value}', kind: monaco.languages.CompletionItemKind.Operator },
          { label: '$in',  doc: 'Matches any of the values specified in an array', text: '\\$in: [${1}]', kind: monaco.languages.CompletionItemKind.Operator },
          { label: '$nin', doc: 'Matches none of the values specified in an array', text: '\\$nin: [${1}]', kind: monaco.languages.CompletionItemKind.Operator },
          // ── Logical ──
          { label: '$and', doc: 'Joins query clauses with a logical AND', text: '\\$and: [{ ${1} }, { ${2} }]', kind: monaco.languages.CompletionItemKind.Operator },
          { label: '$or',  doc: 'Joins query clauses with a logical OR', text: '\\$or: [{ ${1} }, { ${2} }]', kind: monaco.languages.CompletionItemKind.Operator },
          { label: '$not', doc: 'Inverts the effect of a query expression', text: '\\$not: { ${1} }', kind: monaco.languages.CompletionItemKind.Operator },
          { label: '$nor', doc: 'Joins query clauses with a logical NOR', text: '\\$nor: [{ ${1} }, { ${2} }]', kind: monaco.languages.CompletionItemKind.Operator },
          // ── Element ──
          { label: '$exists', doc: 'Matches documents that have the specified field', text: '\\$exists: ${1:true}', kind: monaco.languages.CompletionItemKind.Operator },
          { label: '$type',   doc: 'Selects documents if a field is of the specified type', text: '\\$type: "${1:string}"', kind: monaco.languages.CompletionItemKind.Operator },
          // ── Array ──
          { label: '$all',       doc: 'Matches arrays that contain all specified elements', text: '\\$all: [${1}]', kind: monaco.languages.CompletionItemKind.Operator },
          { label: '$elemMatch', doc: 'Matches documents that contain an array element matching all conditions', text: '\\$elemMatch: { ${1} }', kind: monaco.languages.CompletionItemKind.Operator },
          { label: '$size',      doc: 'Matches arrays with the specified number of elements', text: '\\$size: ${1:1}', kind: monaco.languages.CompletionItemKind.Operator },
          // ── Evaluation ──
          { label: '$regex',  doc: 'Selects documents matching a regular expression', text: '\\$regex: /${1:pattern}/${2:flags}', kind: monaco.languages.CompletionItemKind.Operator },
          { label: '$expr',   doc: 'Use aggregation expressions within the query language', text: '\\$expr: { ${1} }', kind: monaco.languages.CompletionItemKind.Operator },
          // ── Update ──
          { label: '$set',      doc: 'Sets the value of a field', text: '\\$set: { ${1:field}: ${2:value} }', kind: monaco.languages.CompletionItemKind.Function },
          { label: '$unset',    doc: 'Removes the specified field from a document', text: '\\$unset: { ${1:field}: "" }', kind: monaco.languages.CompletionItemKind.Function },
          { label: '$inc',      doc: 'Increments the value of a field by a specified amount', text: '\\$inc: { ${1:field}: ${2:1} }', kind: monaco.languages.CompletionItemKind.Function },
          { label: '$push',     doc: 'Adds an element to an array', text: '\\$push: { ${1:field}: ${2:value} }', kind: monaco.languages.CompletionItemKind.Function },
          { label: '$pull',     doc: 'Removes all elements from an array that match a condition', text: '\\$pull: { ${1:field}: ${2:value} }', kind: monaco.languages.CompletionItemKind.Function },
          { label: '$addToSet', doc: 'Adds a value to an array only if it doesn\'t already exist', text: '\\$addToSet: { ${1:field}: ${2:value} }', kind: monaco.languages.CompletionItemKind.Function },
          { label: '$rename',   doc: 'Renames a field', text: '\\$rename: { "${1:oldName}": "${2:newName}" }', kind: monaco.languages.CompletionItemKind.Function },
          { label: '$min',      doc: 'Updates the field if the specified value is less than the existing value', text: '\\$min: { ${1:field}: ${2:value} }', kind: monaco.languages.CompletionItemKind.Function },
          { label: '$max',      doc: 'Updates the field if the specified value is greater than the existing value', text: '\\$max: { ${1:field}: ${2:value} }', kind: monaco.languages.CompletionItemKind.Function },
          { label: '$mul',      doc: 'Multiplies the value of a field by a specified amount', text: '\\$mul: { ${1:field}: ${2:value} }', kind: monaco.languages.CompletionItemKind.Function },
          // ── Aggregation stages ──
          { label: '$match',   doc: 'Filters documents to pass only matching documents to the next stage', text: '\\$match: { ${1} }', kind: monaco.languages.CompletionItemKind.Module },
          { label: '$group',   doc: 'Groups documents by a specified expression', text: '\\$group: { _id: ${1:null}, ${2:field}: { \\$sum: ${3:1} } }', kind: monaco.languages.CompletionItemKind.Module },
          { label: '$project', doc: 'Reshapes documents by including, excluding, or adding fields', text: '\\$project: { ${1:field}: 1 }', kind: monaco.languages.CompletionItemKind.Module },
          { label: '$sort',    doc: 'Sorts all documents', text: '\\$sort: { ${1:field}: ${2:-1} }', kind: monaco.languages.CompletionItemKind.Module },
          { label: '$limit',   doc: 'Limits the number of documents passed to the next stage', text: '\\$limit: ${1:20}', kind: monaco.languages.CompletionItemKind.Module },
          { label: '$skip',    doc: 'Skips over the specified number of documents', text: '\\$skip: ${1:0}', kind: monaco.languages.CompletionItemKind.Module },
          { label: '$lookup',  doc: 'Performs a left outer join to another collection', text: '\\$lookup: {\n\tfrom: "${1:collection}",\n\tlocalField: "${2:field}",\n\tforeignField: "${3:_id}",\n\tas: "${4:result}"\n}', kind: monaco.languages.CompletionItemKind.Module },
          { label: '$unwind',  doc: 'Deconstructs an array field into a document per element', text: '\\$unwind: "\\$${1:field}"', kind: monaco.languages.CompletionItemKind.Module },
          { label: '$count',   doc: 'Returns a count of the documents at this stage', text: '\\$count: "${1:total}"', kind: monaco.languages.CompletionItemKind.Module },
          { label: '$out',     doc: 'Writes the result of the pipeline to a collection', text: '\\$out: "${1:collection}"', kind: monaco.languages.CompletionItemKind.Module },
          { label: '$merge',   doc: 'Merges the result of the pipeline into a collection', text: '\\$merge: { into: "${1:collection}" }', kind: monaco.languages.CompletionItemKind.Module },
          { label: '$bucket',  doc: 'Categorizes documents into groups (buckets)', text: '\\$bucket: {\n\tgroupBy: "\\$${1:field}",\n\tboundaries: [${2:0, 100, 200}],\n\tdefault: "Other"\n}', kind: monaco.languages.CompletionItemKind.Module },
          { label: '$facet',   doc: 'Process multiple aggregation pipelines in a single stage', text: '\\$facet: {\n\t${1:output}: [{ ${2} }]\n}', kind: monaco.languages.CompletionItemKind.Module },
          { label: '$replaceRoot', doc: 'Replaces the document with the specified embedded document', text: '\\$replaceRoot: { newRoot: "\\$${1:field}" }', kind: monaco.languages.CompletionItemKind.Module },
          // ── Accumulators (inside $group) ──
          { label: '$sum',   doc: 'Calculates the sum', text: '\\$sum: ${1:1}', kind: monaco.languages.CompletionItemKind.Operator },
          { label: '$avg',   doc: 'Calculates the average', text: '\\$avg: "\\$${1:field}"', kind: monaco.languages.CompletionItemKind.Operator },
          { label: '$first', doc: 'Returns the first value in a group', text: '\\$first: "\\$${1:field}"', kind: monaco.languages.CompletionItemKind.Operator },
          { label: '$last',  doc: 'Returns the last value in a group', text: '\\$last: "\\$${1:field}"', kind: monaco.languages.CompletionItemKind.Operator },
        ]
        return {
          suggestions: operators.map((op, i) => ({
            label: op.label,
            filterText: op.label.slice(1),
            kind: op.kind,
            documentation: op.doc,
            insertText: op.text,
            insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            range,
            sortText: String(i).padStart(3, '0'),
          })),
        }
      },
    })

    // Register chained method completions (.sort, .limit, .skip after find/aggregate)
    monaco.languages.registerCompletionItemProvider('javascript', {
      triggerCharacters: ['.'],
      provideCompletionItems(model, position) {
        const textBefore = model.getValueInRange({
          startLineNumber: 1, startColumn: 1,
          endLineNumber: position.lineNumber, endColumn: position.column,
        })
        if (!/\.\s*(find|findOne|aggregate|sort|limit|skip)\s*\(/.test(textBefore)) return { suggestions: [] }

        const word = model.getWordUntilPosition(position)
        const range = {
          startLineNumber: position.lineNumber, endLineNumber: position.lineNumber,
          startColumn: word.startColumn, endColumn: word.endColumn,
        }
        const methods = [
          { label: 'sort',  doc: 'Sort results by field(s)', text: 'sort({ ${1:field}: ${2:-1} })' },
          { label: 'limit', doc: 'Limit the number of results', text: 'limit(${1:20})' },
          { label: 'skip',  doc: 'Skip a number of results', text: 'skip(${1:0})' },
        ]
        return {
          suggestions: methods.map(m => ({
            label: m.label,
            kind: monaco.languages.CompletionItemKind.Method,
            documentation: m.doc,
            insertText: m.text,
            insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            range,
          })),
        }
      },
    })

    // Register collection name completions (inside getCollection("...") or db.xxx)
    // This is a static registration; the actual collection list is updated dynamically
    // via _globalCollectionDisposable (see updateCollectionCompletions below).

    // ── AI Inline Completions ──────────────────────────────────────────────────
    monaco.languages.registerInlineCompletionsProvider('javascript', {
      provideInlineCompletions: async (model, position, _context, token) => {
        if (!settingsStore.aiEnabled) return { items: [] }

        const prefix = model.getValueInRange({
          startLineNumber: 1, startColumn: 1,
          endLineNumber: position.lineNumber, endColumn: position.column,
        })
        const totalLines = model.getLineCount()
        const suffix = model.getValueInRange({
          startLineNumber: position.lineNumber, startColumn: position.column,
          endLineNumber: totalLines, endColumn: model.getLineMaxColumn(totalLines),
        })

        if (prefix.trim().length < 3) return { items: [] }

        const cancelled = await new Promise<boolean>((resolve) => {
          const timer = setTimeout(() => resolve(false), 600)
          token.onCancellationRequested(() => { clearTimeout(timer); resolve(true) })
        })
        if (cancelled || token.isCancellationRequested) return { items: [] }

        const prefixLines = prefix.split('\n')
        const trimmedPrefix = prefixLines.slice(-50).join('\n')
        const suffixLines = suffix.split('\n')
        const trimmedSuffix = suffixLines.slice(0, 10).join('\n')

        try {
          const resp = await aiComplete({
            prefix: trimmedPrefix,
            suffix: trimmedSuffix,
            collection: connStore.activeCollection || undefined,
            db: connStore.activeDb || undefined,
            field_names: _globalCachedFields,
          })

          if (token.isCancellationRequested || !resp.text) return { items: [] }

          return {
            items: [{
              insertText: resp.text,
              range: {
                startLineNumber: position.lineNumber,
                startColumn: position.column,
                endLineNumber: position.lineNumber,
                endColumn: position.column,
              },
            }],
          }
        } catch {
          return { items: [] }
        }
      },
      disposeInlineCompletions: () => {},
    })
  }
  // ── End global providers ──────────────────────────────────────────────────

  // Per-instance: key bindings
  monacoEditor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter, runAtCursor)
  monacoEditor.addCommand(monaco.KeyMod.Shift | monaco.KeyMod.Alt | monaco.KeyCode.KeyF, formatQuery)
  monacoEditor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
    editorStore.saveTabAt(props.tabIndex)
    toast('Script saved', 'info')
  })
  monacoEditor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyS, () => {
    editorStore.saveAll()
    toast('All scripts saved', 'info')
  })

  // Per-instance: content change handler
  monacoEditor.onDidChangeModelContent(() => {
    editorStore.setTabContent(props.tabIndex, monacoEditor!.getValue())
    if (autoSaveTimer) clearTimeout(autoSaveTimer)
    autoSaveTimer = setTimeout(() => editorStore.saveTabAt(props.tabIndex), 2000)
  })
})

onBeforeUnmount(() => {
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
  if (editorStore.activeTabIndex !== props.tabIndex) return  // only active instance handles this
  editorStore.pendingExec = null

  const current = monacoEditor.getValue()
  const appended = (current.trimEnd() ? current.trimEnd() + '\n\n' : '') + query + '\n'
  monacoEditor.setValue(appended)
  editorStore.setTabContent(props.tabIndex, appended)

  const lineCount = monacoEditor.getModel()?.getLineCount() ?? 1
  monacoEditor.revealLine(lineCount)

  const stmt = query.replace(/;\s*$/, '')
  await executeStatements([stmt])
})

// ── Autocomplete: update when collection changes or tab becomes active ────────
watch(() => connStore.activeCollection, async (col) => {
  if (editorStore.activeTabIndex !== props.tabIndex) return
  const conn = connStore.activeConn
  const db = connStore.activeDb
  if (!col || !conn || !db) return
  try {
    const fields = await getFieldPaths(conn, db, col)
    _globalCachedFields = fields
    updateCompletions(fields)
  } catch { /* ignore */ }
})

// (Tab activation: field + collection completions are refreshed in the watcher below updateCollectionCompletions)

function updateCompletions(fields: string[]) {
  _globalFieldDisposable?.dispose()
  _globalFieldDisposable = monaco.languages.registerCompletionItemProvider('javascript', {
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

// ── Collection name completions ──────────────────────────────────────────────
function updateCollectionCompletions(collections: string[]) {
  _globalCollectionDisposable?.dispose()
  _globalCollectionDisposable = monaco.languages.registerCompletionItemProvider('javascript', {
    triggerCharacters: ['"', "'"],
    provideCompletionItems(model, position) {
      // Check if cursor is inside getCollection("...") context
      const lineContent = model.getLineContent(position.lineNumber)
      const textBefore = lineContent.substring(0, position.column - 1)
      if (!/getCollection\s*\(\s*["'][^"']*$/.test(textBefore)) return { suggestions: [] }

      // Find the start of the string (after the quote)
      const quoteMatch = textBefore.match(/getCollection\s*\(\s*["']([^"']*)$/)
      const partialText = quoteMatch?.[1] ?? ''
      const startColumn = position.column - partialText.length

      const range = {
        startLineNumber: position.lineNumber,
        endLineNumber: position.lineNumber,
        startColumn,
        endColumn: position.column,
      }
      return {
        suggestions: collections.map(c => ({
          label: c,
          kind: monaco.languages.CompletionItemKind.Module,
          documentation: `Collection: ${c}`,
          insertText: c,
          range,
        })),
      }
    },
  })
}

// Update collection completions when DB changes
watch(() => connStore.activeDb, async (db) => {
  if (editorStore.activeTabIndex !== props.tabIndex) return
  const conn = connStore.activeConn
  if (!db || !conn) return
  // Try tree cache first, otherwise fetch
  const cached = connStore.tree[conn.id]?.expandedDbs[db]
  if (cached) {
    updateCollectionCompletions(cached)
  } else {
    try {
      const cols = await listCollections(conn, db)
      updateCollectionCompletions(cols)
    } catch { /* ignore */ }
  }
})

// When this tab becomes active, also refresh collection completions
watch(() => editorStore.activeTabIndex, async (idx) => {
  if (idx !== props.tabIndex) return
  const tab = editorStore.tabs[props.tabIndex]
  if (!tab?.connId || !tab.dbName) return
  const conn = connStore.connections.find(c => c.id === tab.connId)
  if (!conn) return

  // Refresh collections
  const cached = connStore.tree[conn.id]?.expandedDbs[tab.dbName]
  if (cached) {
    updateCollectionCompletions(cached)
  } else {
    try {
      const cols = await listCollections(conn, tab.dbName)
      updateCollectionCompletions(cols)
    } catch { /* ignore */ }
  }

  // Also refresh fields if collection is set
  if (!tab.collectionName) return
  try {
    const fields = await getFieldPaths(conn, tab.dbName, tab.collectionName)
    _globalCachedFields = fields
    updateCompletions(fields)
  } catch { /* ignore */ }
}, { immediate: false })

// ── Format query ──────────────────────────────────────────────────────────────
function formatQuery() {
  monacoEditor?.getAction('editor.action.formatDocument')?.run()
}

// ── Run query helpers ─────────────────────────────────────────────────────────
async function executeStatements(stmts: string[]) {
  // Always use the global active context (what the breadcrumb shows).
  // The tab's saved context is only for restoring on tab switch, not for execution.
  // After execution, _saveConnContext persists the current context to the tab.
  const conn = connStore.activeConn
  const db = connStore.activeDb
  if (!conn) { toast('Select a connection first', 'error'); return }
  if (!db) { toast('Select a database first', 'error'); return }
  if (!stmts.length) { toast('No valid statement found', 'error'); return }

  // Ensure connection is alive before executing (auto-reconnect if needed)
  const alive = await connStore.ensureConnected(conn.id)
  if (!alive) { toast('Could not connect to server', 'error'); return }

  editorStore.isExecuting = true
  const label = stmts.length === 1 ? 'Executing…' : `Executing ${stmts.length} statements…`
  toast(label, 'info')

  const tab = editorStore.tabs[props.tabIndex]
  const limit = tab?.queryLimit ?? 50
  try {
    const items: StatementResult[] = []
    const isSinglePaginatable = stmts.length === 1
      && /\.(find|aggregate)\s*\(/.test(stmts[0])
      && !/.limit\s*\(/.test(stmts[0])
    for (let i = 0; i < stmts.length; i++) {
      let stmt = stmts[i]
      if (/\.(find|aggregate)\s*\(/.test(stmt) && !/.limit\s*\(/.test(stmt)) {
        stmt = `${stmt}.limit(${limit})`
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
    const stmts = extractAllStatements(selText.trim()).filter(Boolean)
    await executeStatements(stmts)
    return
  }
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

function stripAllComments(s: string): string {
  return s
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .split('\n').map(l => l.replace(/\/\/.*$/, '')).join('\n')
}

function extractStatementAtCursor(text: string, offset: number): string {
  // Replace both line and block comments with spaces to preserve char positions
  let src = text.replace(/\/\/[^\n]*/g, m => ' '.repeat(m.length))
  src = src.replace(/\/\*[\s\S]*?\*\//g, m => ' '.repeat(m.length))

  let start = 0
  let prevStart = 0
  let lastSemi = -1
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

  const raw = text.slice(start, end)
  const result = stripAllComments(raw).trim()
  if (!result && lastSemi >= 0) {
    return stripAllComments(text.slice(prevStart, lastSemi)).trim()
  }
  return result
}

function extractAllStatements(text: string): string[] {
  // Strip block comments first, then line comments
  const src = text
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .split('\n').map(l => l.replace(/\/\/.*$/, '')).join('\n')
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
      <button class="btn-ghost" style="font-size:11px" @click="editorStore.saveTabAt(props.tabIndex)">Save</button>
    </div>

    <!-- Monaco container -->
    <div ref="containerEl" class="monaco-container" />
  </div>
</template>

<style scoped>
.editor-root { display: flex; flex-direction: column; height: 100%; overflow: hidden; background: var(--bg); }

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
