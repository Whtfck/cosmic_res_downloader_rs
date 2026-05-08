<script setup lang="ts">
import { computed } from "vue";
import type { FileItem } from "../types";

const props = defineProps<{ files: FileItem[] }>();

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
</script>

<template>
  <div class="file-list">
    <div class="header">文件列表 ({{ files.length }})</div>
    <div class="scroll-area">
      <div v-for="[group, items] in grouped" :key="group" class="group">
        <div class="group-header">{{ group }} <span class="count">({{ items.length }})</span></div>
        <div v-for="item in items" :key="item.filename" class="file-item">
          <span class="status">{{ statusIcon(item.status) }}</span>
          <span class="name-col">
            <span class="filename">{{ item.filename }}</span>
            <span class="md5">{{ item.md5.slice(0, 8) }}</span>
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
  </div>
</template>

<style scoped>
.file-list {
  flex: 1;
  background: #16213e;
  border-radius: 8px;
  display: flex;
  flex-direction: column;
  min-width: 0;
  overflow: hidden;
}

.header {
  padding: 8px 12px;
  color: #7c83ff;
  font-size: 0.85rem;
  font-weight: 600;
  border-bottom: 1px solid #2a2a4a;
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
  color: #e94560;
  font-weight: 600;
  font-size: 0.78rem;
  padding: 4px 12px 2px;
}

.group-header .count {
  color: #8892b0;
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
  background: #0f3460;
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
  color: #e0e0e0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.md5 {
  color: #8892b0;
  font-family: monospace;
  font-size: 0.7rem;
  flex-shrink: 0;
}

.pct {
  flex-shrink: 0;
  font-size: 0.75rem;
  color: #7c83ff;
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
  color: #8892b0;
  font-family: monospace;
}

.size.done { color: #4caf50; }

.pct.done { color: #4caf50; }
.pct.fail { color: #ff6b6b; }
.pct.unzip { color: #ffa726; }
</style>
