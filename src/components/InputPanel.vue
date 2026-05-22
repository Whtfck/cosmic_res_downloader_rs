<script setup lang="ts">
import { ref } from "vue";

interface Header {
  key: string;
  value: string;
}

const emit = defineEmits<{
  parse: [jsonUrl: string, headers: Record<string, string>, filterPattern: string, xmlFilter: boolean];
}>();

defineProps<{ loading: boolean }>();

const DEFAULT_FILTER = String.raw`\d+(?:bk|bak)|(?:bk|bak)\d+`;

const jsonUrl = ref("");
const showAdvanced = ref(false);
const filterPattern = ref(DEFAULT_FILTER);
const enableXmlFilter = ref(true);
const headers = ref<Header[]>([{ key: "", value: "" }]);

function addHeader() {
  headers.value.push({ key: "", value: "" });
}

function removeHeader(index: number) {
  headers.value.splice(index, 1);
  if (headers.value.length === 0) {
    headers.value.push({ key: "", value: "" });
  }
}

function handleSubmit() {
  if (!jsonUrl.value.trim()) return;
  const headerMap: Record<string, string> = {};
  for (const h of headers.value) {
    const k = h.key.trim();
    const v = h.value.trim();
    if (k && v) headerMap[k] = v;
  }
  emit("parse", jsonUrl.value.trim(), headerMap, filterPattern.value, enableXmlFilter.value);
}
</script>

<template>
  <div class="input-panel">
    <div class="url-row">
      <input
        v-model="jsonUrl"
        type="text"
        placeholder="输入 URL（自动追加 /update.json）"
        @keyup.enter="handleSubmit"
      />
      <button @click="handleSubmit" :disabled="loading">
        {{ loading ? "解析中..." : "解析" }}
      </button>
    </div>

    <div class="advanced-section">
      <button class="toggle-btn" @click="showAdvanced = !showAdvanced">
        高级选项 {{ showAdvanced ? "▲" : "▼" }}
      </button>
      <div v-if="showAdvanced" class="advanced-content">
        <div class="filter-row">
          <label>文件过滤（正则）</label>
          <input
            v-model="filterPattern"
            type="text"
            :placeholder="DEFAULT_FILTER"
            class="filter-input"
          />
          <span class="hint">匹配的文件名将被跳过，多后缀文件始终过滤</span>
        </div>

        <label class="xml-filter-opt">
          <input type="checkbox" v-model="enableXmlFilter" />
          XML过滤（只下载/解压XML中列出的zip）
        </label>

        <div class="headers-block">
          <label>请求头</label>
          <div v-for="(h, i) in headers" :key="i" class="header-row">
            <input
              v-model="h.key"
              type="text"
              placeholder="Header Name"
              class="header-key"
            />
            <input
              v-model="h.value"
              type="text"
              placeholder="Header Value"
              class="header-val"
            />
            <button class="remove-btn" @click="removeHeader(i)">−</button>
          </div>
          <button class="add-btn" @click="addHeader">+ 添加</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.input-panel {
  flex-shrink: 0;
}

.url-row {
  display: flex;
  gap: 8px;
}

input {
  flex: 1;
  padding: 8px 12px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--surface);
  color: var(--text);
  font-size: 0.9rem;
  outline: none;
}

input:focus {
  border-color: var(--accent);
}

button {
  padding: 8px 24px;
  background: var(--accent);
  color: #fff;
  border: none;
  border-radius: 6px;
  font-size: 0.9rem;
  cursor: pointer;
}

button:hover:not(:disabled) { opacity: 0.85; }
button:disabled { opacity: 0.5; cursor: not-allowed; }

.advanced-section {
  margin-top: 8px;
}

.toggle-btn {
  background: transparent;
  color: var(--text-dim);
  padding: 4px 0;
  font-size: 0.78rem;
  border: none;
  cursor: pointer;
}

.toggle-btn:hover { color: var(--text); }

.advanced-content {
  margin-top: 6px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.filter-row {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.filter-row label, .headers-block label {
  font-size: 0.78rem;
  color: var(--text-dim);
}

.filter-input {
  font-family: "Cascadia Code", "Fira Code", monospace;
  font-size: 0.8rem;
  padding: 5px 8px;
}

.hint {
  font-size: 0.7rem;
  color: var(--text-muted);
}

.headers-block {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.header-row {
  display: flex;
  gap: 6px;
}

.header-key {
  width: 140px;
  flex: none;
  padding: 5px 8px;
  font-size: 0.8rem;
}

.header-val {
  flex: 1;
  padding: 5px 8px;
  font-size: 0.8rem;
}

.remove-btn {
  width: 28px;
  padding: 5px;
  background: var(--error-bg);
  color: var(--error-text);
  border: none;
  border-radius: 4px;
  font-size: 1rem;
  cursor: pointer;
  flex-shrink: 0;
}

.remove-btn:hover { opacity: 0.8; }

.add-btn {
  align-self: flex-start;
  padding: 4px 12px;
  background: transparent;
  color: var(--accent);
  border: 1px solid var(--border);
  border-radius: 4px;
  font-size: 0.78rem;
  cursor: pointer;
}

.add-btn:hover { border-color: var(--accent); }

.xml-filter-opt {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 0.78rem;
  color: var(--text-dim);
  cursor: pointer;
}

.xml-filter-opt input {
  width: auto;
  flex: none;
}
</style>
