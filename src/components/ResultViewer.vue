<script setup lang="ts">
import { ref, computed, watch, inject, onMounted, onBeforeUnmount } from 'vue'
import { useEditorStore } from '../stores/editor'
import { useConnectionsStore } from '../stores/connections'
import { executeQuery } from '../lib/tauri'

const editorStore = useEditorStore()
const connStore = useConnectionsStore()
const toast = inject<(msg: string, type?: 'success' | 'error' | 'info') => void>('toast')!
const activeTab = ref<'table' | 'json' | 'tree'>('table')

const results = computed(() => editorStore.results)
const result = computed(() => editorStore.result)
const rows = computed((): Record<string, unknown>[] => (result.value?.data ?? []) as Record<string, unknown>[])
const columns = computed(() => {
  if (!rows.value.length) return []
  const keys = new Set<string>()
  rows.value.slice(0, 50).forEach(row => Object.keys(row).forEach(k => keys.add(k)))
  return Array.from(keys)
})

const jsonText = computed(() =>
  result.value?.data ? JSON.stringify(result.value.data, null, 2) : ''
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
})

const copiedKey = ref<string | null>(null)
function copyCell(rowIdx: number, col: string, value: unknown) {
  const str = value === null || value === undefined ? '' : typeof value === 'object' ? JSON.stringify(value) : String(value)
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
  const conn = connStore.activeConn
  const db = connStore.activeDb
  if (!conn || !db) return
  await editorStore.paginate(conn, db, editorStore.currentPage + direction)
}

function onKeyDown(e: KeyboardEvent) {
  if (e.key === 'Escape') selectedDoc.value = null
}
onMounted(() => document.addEventListener('keydown', onKeyDown))
onBeforeUnmount(() => document.removeEventListener('keydown', onKeyDown))

// ── Inline document edit ──────────────────────────────────────────────────────
const editingRowIdx = ref<number | null>(null)
const editedCells = ref<Record<string, string>>({})
const savingRow = ref(false)

function startEdit(rowIdx: number, col: string, value: unknown) {
  if (col === '_id') return // never edit _id
  editingRowIdx.value = rowIdx
  editedCells.value = {}
  const str = value === null || value === undefined ? '' : typeof value === 'object' ? JSON.stringify(value) : String(value)
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
  // string — wrap in quotes, escape internals
  return JSON.stringify(s)
}

async function saveRow(rowIdx: number) {
  const conn = connStore.activeConn
  const db = connStore.activeDb
  const col = connStore.activeCollection
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
  if (typeof v === 'string') return 'val-str'
  return ''
}
function displayValue(v: unknown): string {
  if (v === null) return 'null'
  if (typeof v === 'string') return `"${v}"`
  return String(v)
}
</script>

<template>
  <div class="viewer-root">
    <div class="viewer-bar">
      <div class="view-tabs">
        <button :class="['view-tab', { active: activeTab === 'table' }]" @click="activeTab = 'table'">Table</button>
        <button :class="['view-tab', { active: activeTab === 'json' }]" @click="activeTab = 'json'">JSON</button>
        <button :class="['view-tab', { active: activeTab === 'tree' }]" @click="activeTab = 'tree'">Tree</button>
      </div>
      <span class="viewer-spacer" />
      <template v-if="result?.success && rows.length">
        <span class="row-count">{{ rows.length }} rows</span>
        <button class="btn-ghost" style="font-size:11px" @click="exportCSV">↓ CSV</button>
        <button class="btn-ghost" style="font-size:11px" @click="exportJSON">↓ JSON</button>
      </template>
      <template v-if="editorStore.canPaginate && result?.success">
        <div class="page-nav">
          <button class="page-btn" :disabled="editorStore.currentPage === 0 || editorStore.isExecuting" @click="doPaginate(-1)">‹</button>
          <span class="page-label">p.{{ editorStore.currentPage + 1 }}</span>
          <button class="page-btn" :disabled="rows.length < editorStore.queryLimit || editorStore.isExecuting" @click="doPaginate(1)">›</button>
        </div>
      </template>
      <span class="limit-label">Limit</span>
      <select v-model.number="editorStore.queryLimit" class="limit-select" title="Max documents to return">
        <option :value="50">50</option>
        <option :value="100">100</option>
        <option :value="200">200</option>
        <option :value="500">500</option>
      </select>
    </div>

    <!-- Statement tabs (only when multiple statements were run) -->
    <div v-if="results.length > 1" class="stmt-tabs">
      <button
        v-for="(item, i) in results"
        :key="i"
        :class="['stmt-tab', { active: editorStore.activeResultIdx === i }]"
        @click="editorStore.activeResultIdx = i"
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
      <div v-else-if="activeTab === 'table'" class="viewer-content">
        <div v-if="!rows.length" class="viewer-empty">No documents returned</div>
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
                v-for="(row, i) in rows"
                :key="i"
                :class="{ 'row-selected': selectedDocIdx === i && selectedDoc !== null, 'row-editing': editingRowIdx === i }"
                @click="editingRowIdx === null ? selectRow(row, i) : undefined"
              >
                <td class="row-num" @click.stop="selectRow(row, i)">
                  <template v-if="editingRowIdx === i">
                    <button class="edit-save-btn" :disabled="savingRow" title="Save changes" @click.stop="saveRow(i)">{{ savingRow ? '…' : '✓' }}</button>
                    <button class="edit-cancel-btn" title="Cancel" @click.stop="cancelEdit">✕</button>
                  </template>
                  <template v-else>{{ i + 1 }}</template>
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
      <div v-else-if="activeTab === 'json'" class="viewer-content">
        <div v-if="!rows.length" class="viewer-empty">No documents returned</div>
        <div v-else class="json-tree">
          <JsonNode
            v-for="(row, i) in rows"
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
      <div v-else-if="activeTab === 'tree'" class="viewer-content">
        <div v-if="!rows.length" class="viewer-empty">No documents returned</div>
        <div v-else class="tree-view">
          <div v-for="(row, i) in rows" :key="i" class="tree-doc">
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
      const cls = (v: unknown) => {
        if (v === null)             return 'jv-null'
        if (typeof v === 'boolean') return 'jv-bool'
        if (typeof v === 'number')  return 'jv-num'
        if (typeof v === 'string')  return 'jv-str'
        return ''
      }
      const disp = (v: unknown): string => {
        if (v === null) return 'null'
        if (typeof v === 'string') return `"${v}"`
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
    const cls = (v: unknown) => {
      if (v === null)            return 'val-null'
      if (typeof v === 'boolean') return 'val-bool'
      if (typeof v === 'number')  return 'val-num'
      if (typeof v === 'string')  return 'val-str'
      return ''
    }
    const disp = (v: unknown): string => {
      if (v === null) return 'null'
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

/* JSON tree */
.json-tree { padding: 8px 12px; font-family: var(--font-mono); font-size: var(--font-size-ui, 13px); }
.jrow { display: flex; align-items: baseline; gap: 2px; padding: 1px 0; line-height: 1.6; white-space: nowrap; }
.jexpandable { cursor: pointer; border-radius: 2px; }
.jexpandable:hover { background: var(--bg-hover); }
.jcaret { font-size: 9px; color: var(--text-muted); width: 12px; flex-shrink: 0; }
.jk  { color: var(--blue); }
.jp  { color: var(--text-muted); }
.jv-str  { color: var(--green); }
.jv-num  { color: var(--yellow); }
.jv-bool { color: var(--accent); }
.jv-null { color: var(--text-muted); font-style: italic; }

/* Tree */
.tree-view { padding: 6px; }
.tree-doc { margin-bottom: 4px; border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; }
.tree-doc-header {
  display: flex; align-items: center; gap: 4px;
  padding: 4px 8px; background: var(--bg-card); cursor: pointer;
  font-size: 11px; font-weight: 600; color: var(--text-dim);
}
.tree-doc-header:hover { background: var(--bg-hover); }
.tree-row { display: flex; align-items: baseline; gap: 4px; padding: 1px 8px; font-size: var(--font-size-ui, 13px); font-family: var(--font-mono); }
.tree-row.tree-expandable { cursor: pointer; }
.tree-row.tree-expandable:hover { background: var(--bg-hover); }
.tree-caret { font-size: 9px; color: var(--text-muted); width: 10px; flex-shrink: 0; }
.tree-key  { color: var(--blue); }
.val-str   { color: var(--green); }
.val-num   { color: var(--yellow); }
.val-bool  { color: var(--accent); }
.val-null  { color: var(--text-muted); font-style: italic; }
.val-dim   { color: var(--text-muted); }
</style>
