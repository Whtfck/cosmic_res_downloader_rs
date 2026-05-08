<script setup lang="ts">
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import type { FileItem, DownloadProgress, RunMode } from "../types";

const props = defineProps<{ files: FileItem[]; headers: Record<string, string> }>();
const emit = defineEmits<{
  update: [local_path: string, status: string, progress: number, downloaded_bytes: number, total_bytes: number];
  reset: [];
}>();

const destDir = ref("");
const fileConcurrency = ref(5);
const chunkThreads = ref(1);
const retries = ref(3);
const mode = ref<RunMode>("both");
const running = ref(false);
const speed = ref(0);

const completedCount = computed(
  () => props.files.filter((f) => f.status === "completed" || f.status === "skipped" || f.status === "unzipped" || f.status === "unzipping").length
);

const failedCount = computed(
  () => props.files.filter((f) => f.status === "failed").length
);

const pendingCount = computed(
  () => props.files.filter((f) => f.status === "pending").length
);

const downloadingCount = computed(
  () => props.files.filter((f) => f.status === "downloading").length
);

const unzippingCount = computed(
  () => props.files.filter((f) => f.status === "unzipping").length
);

const overallPercent = computed(() =>
  props.files.length > 0
    ? Math.round((completedCount.value / props.files.length) * 100)
    : 0
);

function formatSpeed(bytes: number): string {
  if (bytes < 1024) return bytes + " B/s";
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB/s";
  return (bytes / (1024 * 1024)).toFixed(1) + " MB/s";
}

async function selectDir() {
  const selected = await open({ directory: true });
  if (selected) {
    destDir.value = selected;
  }
}

let unlistenProgress: (() => void) | null = null;
let unlistenSpeed: (() => void) | null = null;
let unlistenDownloadComplete: (() => void) | null = null;
let unlistenWarning: (() => void) | null = null;

const downloadCompleted = ref(false);
const unzipProgress = ref({ done: 0, total: 0, percent: 0 });
const warnings = ref<string[]>([]);

async function startProcess() {
  if (!destDir.value) return;
  running.value = true;
  speed.value = 0;
  downloadCompleted.value = false;
  unzipProgress.value = { done: 0, total: 0, percent: 0 };
  warnings.value = [];

  unlistenProgress = await listen<DownloadProgress>("download-progress", (event) => {
    const p = event.payload;
    emit("update", p.local_path, p.status, p.progress, p.downloaded_bytes, p.total_bytes);

    // Track unzip progress
    if (p.status === "unzipping") {
      unzipProgress.value.total = props.files.filter(f => f.filename.endsWith(".zip")).length;
    } else if (p.status === "unzipped") {
      unzipProgress.value.done++;
      if (unzipProgress.value.total > 0) {
        unzipProgress.value.percent = Math.round((unzipProgress.value.done / unzipProgress.value.total) * 100);
      }
    }
  });

  unlistenSpeed = await listen<number>("download-speed", (event) => {
    speed.value = event.payload;
  });

  unlistenWarning = await listen<string>("download-warning", (event) => {
    warnings.value.push(event.payload);
  });

  // Listen for download-complete event to fix pending files immediately
  unlistenDownloadComplete = await listen("download-complete", () => {
    downloadCompleted.value = true;
    for (const f of props.files) {
      if (f.status === "pending") {
        emit("update", f.local_path, "completed", 100, 0, 0);
      }
    }
  });

  try {
    await invoke("start_process", {
      tasks: props.files,
      destDir: destDir.value,
      fileConcurrency: fileConcurrency.value,
      chunkThreads: chunkThreads.value,
      retries: retries.value,
      mode: mode.value,
      headers: props.headers,
    });

    // Final fix: mark any remaining "pending" files as "completed"
    for (const f of props.files) {
      if (f.status === "pending") {
        emit("update", f.local_path, "completed", 100, 0, 0);
      }
    }
  } catch (e) {
    console.error("Process failed:", e);
  } finally {
    running.value = false;
    speed.value = 0;
    unlistenProgress?.();
    unlistenSpeed?.();
    unlistenDownloadComplete?.();
    unlistenWarning?.();
    unlistenProgress = null;
    unlistenSpeed = null;
    unlistenDownloadComplete = null;
    unlistenWarning = null;
  }
}

async function handleCancel() {
  await invoke("cancel_process");
}

function handleReset() {
  if (running.value) return;
  emit("reset");
}
</script>


<template>
  <div class="download-panel">
    <div class="row">
      <input :value="destDir" readonly placeholder="下载目录..." class="dir-input" />
      <button @click="selectDir" :disabled="running" class="dir-btn">选择</button>
    </div>

    <div class="row">
      <label class="mode-label">
        模式
        <select v-model="mode" :disabled="running">
          <option value="both">下载并解压</option>
          <option value="download_only">仅下载</option>
          <option value="unzip_only">仅解压</option>
        </select>
      </label>
      <label class="opt">并发 <input v-model.number="fileConcurrency" type="number" min="1" max="20" :disabled="running" /></label>
      <label class="opt">线程 <input v-model.number="chunkThreads" type="number" min="1" max="32" :disabled="running" /></label>
      <label class="opt">重试 <input v-model.number="retries" type="number" min="0" max="10" :disabled="running" /></label>
    </div>

    <div class="row actions">
      <button v-if="!running" class="reset-btn" @click="handleReset">重置</button>
      <button class="start-btn" @click="startProcess" :disabled="running || !destDir">
        {{ running ? "处理中..." : "开始" }}
      </button>
      <button v-if="running" class="stop-btn" @click="handleCancel">停止</button>
    </div>

    <div v-if="completedCount > 0 || running" class="progress-area">
      <div class="section-label">下载进度</div>
      <div class="bar-bg">
        <div class="bar-fill" :style="{ width: overallPercent + '%' }"></div>
      </div>
      <div class="progress-row">
        <span>{{ completedCount }}/{{ files.length }}<span v-if="failedCount > 0" class="fail"> 失败{{ failedCount }}</span></span>
        <span v-if="running && !downloadCompleted" class="speed">{{ formatSpeed(speed) }}</span>
        <span>{{ overallPercent }}%</span>
      </div>
      <div class="progress-row">
        <span class="detail">
          <template v-if="pendingCount > 0 || downloadingCount > 0">
            待下载: {{ pendingCount }} | 下载中: {{ downloadingCount }} |
          </template>
          完成: {{ files.filter(f => f.status === 'completed').length }} | 跳过: {{ files.filter(f => f.status === 'skipped').length }}
        </span>
      </div>
    </div>

    <div v-if="warnings.length > 0" class="warnings">
      <div v-for="(w, i) in warnings" :key="i" class="warn-item">⚠ {{ w }}</div>
    </div>

    <div v-if="downloadCompleted || unzippingCount > 0" class="progress-area">
      <div class="section-label">解压进度</div>
      <div class="bar-bg">
        <div class="bar-fill unzip-bar" :style="{ width: unzipProgress.percent + '%' }"></div>
      </div>
      <div class="progress-row">
        <span>{{ unzipProgress.done }}/{{ unzipProgress.total || files.filter(f => f.filename.endsWith('.zip')).length }}</span>
        <span>{{ unzipProgress.percent }}%</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.download-panel {
  width: 280px;
  background: #16213e;
  padding: 10px 12px;
  border-radius: 8px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  flex-shrink: 0;
}

.row {
  display: flex;
  gap: 6px;
  align-items: center;
}

.dir-input {
  flex: 1;
  padding: 5px 8px;
  border: 1px solid #2a2a4a;
  border-radius: 4px;
  background: #0f3460;
  color: #e0e0e0;
  font-size: 0.8rem;
  outline: none;
  min-width: 0;
}

.dir-btn {
  padding: 5px 10px;
  background: #533483;
  color: #fff;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-size: 0.78rem;
  flex-shrink: 0;
}

.dir-btn:disabled { opacity: 0.5; cursor: not-allowed; }

.mode-label, .opt {
  display: flex;
  align-items: center;
  gap: 4px;
  color: #8892b0;
  font-size: 0.75rem;
}

.mode-label select, .opt input {
  padding: 2px 4px;
  border: 1px solid #2a2a4a;
  border-radius: 3px;
  background: #0f3460;
  color: #e0e0e0;
  font-size: 0.75rem;
}

.mode-label select { cursor: pointer; }
.opt input { width: 36px; text-align: center; }

.actions {
  justify-content: flex-end;
  gap: 8px;
}

.start-btn {
  padding: 6px 20px;
  background: #e94560;
  color: #fff;
  border: none;
  border-radius: 4px;
  font-size: 0.85rem;
  font-weight: 600;
  cursor: pointer;
}

.start-btn:hover:not(:disabled) { opacity: 0.85; }
.start-btn:disabled { opacity: 0.5; cursor: not-allowed; }

.stop-btn {
  padding: 6px 16px;
  background: #666;
  color: #fff;
  border: none;
  border-radius: 4px;
  font-size: 0.85rem;
  cursor: pointer;
}

.stop-btn:hover { background: #888; }

.reset-btn {
  padding: 6px 16px;
  background: transparent;
  color: #8892b0;
  border: 1px solid #2a2a4a;
  border-radius: 4px;
  font-size: 0.85rem;
  cursor: pointer;
}

.reset-btn:hover { border-color: #8892b0; color: #e0e0e0; }

.progress-area { margin-top: 2px; }

.section-label {
  font-size: 0.7rem;
  color: #5a5a7a;
  margin-bottom: 2px;
}

.bar-bg {
  height: 6px;
  background: #0f3460;
  border-radius: 3px;
  overflow: hidden;
}

.bar-fill {
  height: 100%;
  background: linear-gradient(90deg, #7c83ff, #e94560);
  border-radius: 3px;
  transition: width 0.3s ease;
}

.unzip-bar {
  background: linear-gradient(90deg, #4caf50, #8bc34a);
}

.progress-row {
  margin-top: 3px;
  font-size: 0.72rem;
  color: #8892b0;
  display: flex;
  justify-content: space-between;
}

.speed { color: #7c83ff; }
.fail { color: #ff6b6b; }
.detail { font-size: 0.68rem; color: #5a5a7a; }

.warnings {
  margin-top: 4px;
}
.warn-item {
  font-size: 0.68rem;
  color: #ffa726;
  padding: 2px 0;
}
</style>
