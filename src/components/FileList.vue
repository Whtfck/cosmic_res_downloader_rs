<script setup lang="ts">
import { computed, ref } from "vue";
import type { FileItem } from "../types";

const props = defineProps<{
  files: FileItem[];
  ignored: string[];
  forceDownload: string[];
}>();

const emit = defineEmits<{
  ignore: [filename: string];
  force: [filename: string];
  resetFile: [filename: string];
}>();

const ignoredSet = computed(() => new Set(props.ignored));
const forceSet = computed(() => new Set(props.forceDownload));

const grouped = computed(() => {
  const map = new Map<string, FileItem[]>();
  for (const f of props.files) {
    if (!map.has(f.group)) map.set(f.group, []);
    map.get(f.group)!.push(f);
  }
  return map;
});

function statusIcon(status: FileItem["status"]) {
  switch (status) {
    case "pending": return "⏳";
    case "downloading": return "⬇️";
    case "completed": return "✅";
    case "skipped": return "⏭️";
    case "failed": return "❌";
    case "unzipping": return "📦";
    case "unzipped": return "📂";
  }
}

function formatSize(bytes: number): string {
  if (bytes === 0) return "";
  if (bytes < 1024) return bytes + " B";
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
  if (bytes < 1024 * 1024 * 1024) return (bytes / (1024 * 1024)).toFixed(1) + " MB";
  return (bytes / (1024 * 1024 * 1024)).toFixed(2) + " GB";
}

// Context menu
const ctxVisible = ref(false);
const ctxX = ref(0);
const ctxY = ref(0);
const ctxFilename = ref("");

function onContextMenu(e: MouseEvent, filename: string) {
  e.preventDefault();
  ctxFilename.value = filename;
  ctxX.value = e.clientX;
  ctxY.value = e.clientY;
  ctxVisible.value = true;
}

function closeCtx() {
  ctxVisible.value = false;
}

function ctxIgnore() {
  emit("ignore", ctxFilename.value);
  closeCtx();
}

function ctxForce() {
  emit("force", ctxFilename.value);
  closeCtx();
}

function ctxReset() {
  emit("resetFile", ctxFilename.value);
  closeCtx();
}

function isIgnored(filename: string): boolean {
  return ignoredSet.value.has(filename);
}

function isForced(filename: string): boolean {
  return forceSet.value.has(filename);
}
</script>

<template>
  <div class="file-list" @click="closeCtx">
    <div class="header">文件列表 ({{ files.length }})</div>
    <div class="scroll-area">
      <div v-for="[group, items] in grouped" :key="group" class="group">
        <div class="group-header">{{ group }} <span class="count">({{ items.length }})</span></div>
        <div
          v-for="item in items"
          :key="item.filename"
          class="file-item"
          :class="{ ignored: isIgnored(item.filename), forced: isForced(item.filename) }"
          @contextmenu="onContextMenu($event, item.filename)"
        >
          <span class="status">{{ statusIcon(item.status) }}</span>
          <span class="name-col">
            <span class="filename">{{ item.filename }}</span>
            <span class="md5">{{ item.md5.slice(0, 8) }}</span>
            <span v-if="isIgnored(item.filename)" class="tag tag-ignore">IGNORE</span>
            <span v-else-if="isForced(item.filename)" class="tag tag-force">FORCE</span>
          </span>
          <span v-if="item.status === 'downloading'" class="size-info">
            <span class="size">{{ formatSize(item.downloaded_bytes) }}/{{ formatSize(item.total_bytes) }}</span>
            <span class="pct">{{ item.progress }}%</span>
          </span>
          <span v-else-if="item.status === 'unzipping'" class="pct unzip">...</span>
          <span v-else-if="item.status === 'completed'" class="size-info">
            <span class="size done">{{ formatSize(item.total_bytes) }}</span>
            <span class="pct done">OK</span>
          </span>
          <span v-else-if="item.status === 'skipped'" class="size-info">
            <span class="size done">{{ formatSize(item.total_bytes) }}</span>
            <span class="pct done">SKIP</span>
          </span>
          <span v-else-if="item.status === 'unzipped'" class="pct done">OK</span>
          <span v-else-if="item.status === 'failed'" class="pct fail">FAIL</span>
        </div>
      </div>
    </div>

    <Teleport to="body">
      <div v-if="ctxVisible" class="ctx-menu" :style="{ left: ctxX + 'px', top: ctxY + 'px' }">
        <div class="ctx-item" @click="ctxIgnore">忽略（跳过下载）</div>
        <div class="ctx-item" @click="ctxForce">强制下载（忽略MD5）</div>
        <div class="ctx-item" @click="ctxReset">重置</div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.file-list {
  flex: 1;
  background: var(--surface);
  border-radius: 8px;
  display: flex;
  flex-direction: column;
  min-width: 0;
  overflow: hidden;
}

.header {
  padding: 8px 12px;
  color: var(--accent);
  font-size: 0.85rem;
  font-weight: 600;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.scroll-area {
  flex: 1;
  overflow-y: auto;
  padding: 4px 0;
}

.group {
  margin-bottom: 2px;
}

.group-header {
  color: var(--accent2);
  font-weight: 600;
  font-size: 0.78rem;
  padding: 4px 12px 2px;
}

.group-header .count {
  color: var(--text-dim);
  font-weight: 400;
}

.file-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 2px 12px;
  font-size: 0.78rem;
}

.file-item:hover {
  background: var(--surface-hover);
}

.file-item.ignored {
  opacity: 0.4;
}

.file-item.forced .filename {
  color: #ffa726;
}

.status {
  flex-shrink: 0;
  width: 16px;
  text-align: center;
  font-size: 0.7rem;
}

.name-col {
  flex: 1;
  display: flex;
  gap: 8px;
  align-items: baseline;
  min-width: 0;
}

.filename {
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.md5 {
  color: var(--text-dim);
  font-family: monospace;
  font-size: 0.7rem;
  flex-shrink: 0;
}

.tag {
  font-size: 0.6rem;
  font-weight: 600;
  padding: 0 4px;
  border-radius: 2px;
  flex-shrink: 0;
}

.tag-ignore {
  background: #666;
  color: #ccc;
}

.tag-force {
  background: #e65100;
  color: #fff;
}

.pct {
  flex-shrink: 0;
  font-size: 0.75rem;
  color: var(--accent);
  min-width: 32px;
  text-align: right;
}

.size-info {
  display: flex;
  gap: 6px;
  align-items: center;
  flex-shrink: 0;
}

.size {
  font-size: 0.7rem;
  color: var(--text-dim);
  font-family: monospace;
}

.size.done { color: #4caf50; }

.pct.done { color: #4caf50; }
.pct.fail { color: #ff6b6b; }
.pct.unzip { color: #ffa726; }
</style>

<style>
.ctx-menu {
  position: fixed;
  background: var(--surface, #16213e);
  border: 1px solid var(--border, #2a2a4a);
  border-radius: 6px;
  padding: 4px 0;
  z-index: 999;
  min-width: 180px;
  box-shadow: 0 4px 12px rgba(0,0,0,0.4);
}

.ctx-item {
  padding: 6px 14px;
  font-size: 0.8rem;
  color: var(--text, #e0e0e0);
  cursor: pointer;
}

.ctx-item:hover {
  background: var(--surface-hover, #0f3460);
}
</style>
