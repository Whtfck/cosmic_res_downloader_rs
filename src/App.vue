<script setup lang="ts">
import { ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import InputPanel from "./components/InputPanel.vue";
import EnvInfoPanel from "./components/EnvInfoPanel.vue";
import FileList from "./components/FileList.vue";
import DownloadPanel from "./components/DownloadPanel.vue";
import type { FileItem, EnvInfo, FetchResult } from "./types";

const appWindow = getCurrentWindow();
const theme = ref<"dark" | "light">("dark");
watch(theme, (v) => {
  document.documentElement.classList.toggle("light", v === "light");
  appWindow.setTheme(v === "light" ? "light" : "dark");
}, { immediate: true });

const envInfo = ref<EnvInfo | null>(null);
const fileList = ref<FileItem[]>([]);
const loading = ref(false);
const error = ref("");
const currentHeaders = ref<Record<string, string>>({});
const xmlFilter = ref(true);
const currentFilterPattern = ref("");
const ignoredFiles = ref<string[]>([]);
const forceDownloadFiles = ref<string[]>([]);

async function handleParse(jsonUrl: string, headers: Record<string, string>, filterPattern: string, enableXmlFilter: boolean) {
  loading.value = true;
  error.value = "";
  currentHeaders.value = headers;
  xmlFilter.value = enableXmlFilter;
  currentFilterPattern.value = filterPattern;
  try {
    const result = await invoke<FetchResult>("fetch_json", { url: jsonUrl, headers });

    const env = result.json["env"] as Record<string, unknown> | undefined;
    if (env) {
      envInfo.value = {
        number: (env["number"] as string) || "",
        name: (env["name"] as string) || "",
        zk_address: (env["zkAddress"] as string) || "",
      };
    }

    const items = await invoke<FileItem[]>("parse_file_list", {
      json: result.json,
      baseUrl: result.base_url,
      filterPattern,
    });
    fileList.value = items;
  } catch (e) {
    error.value = String(e);
    envInfo.value = null;
    fileList.value = [];
  } finally {
    loading.value = false;
  }
}

function handleDownloadUpdate(local_path: string, status: string, progress: number, downloaded_bytes: number, total_bytes: number) {
  const item = fileList.value.find((f) => f.local_path === local_path);
  if (item) {
    item.status = status as FileItem["status"];
    item.progress = progress;
    item.downloaded_bytes = downloaded_bytes;
    item.total_bytes = total_bytes;
  }
}

function handleIgnore(filename: string) {
  if (!ignoredFiles.value.includes(filename)) {
    ignoredFiles.value = [...ignoredFiles.value, filename];
  }
  forceDownloadFiles.value = forceDownloadFiles.value.filter(f => f !== filename);
}

function handleForce(filename: string) {
  if (!forceDownloadFiles.value.includes(filename)) {
    forceDownloadFiles.value = [...forceDownloadFiles.value, filename];
  }
  ignoredFiles.value = ignoredFiles.value.filter(f => f !== filename);
}

function handleResetFile(filename: string) {
  ignoredFiles.value = ignoredFiles.value.filter(f => f !== filename);
  forceDownloadFiles.value = forceDownloadFiles.value.filter(f => f !== filename);
}

function handleReset() {
  envInfo.value = null;
  fileList.value = [];
  error.value = "";
  currentHeaders.value = {};
  ignoredFiles.value = [];
  forceDownloadFiles.value = [];
}
</script>

<template>
  <div class="app">
    <button class="theme-toggle" @click="theme = theme === 'dark' ? 'light' : 'dark'" :title="theme === 'dark' ? '切换亮色' : '切换暗色'">
      {{ theme === "dark" ? "☀" : "☾" }}
    </button>
    <h1>Cosmic Resource Downloader</h1>
    <InputPanel @parse="handleParse" :loading="loading" />
    <div v-if="error" class="error">{{ error }}</div>
    <div class="main-area">
      <EnvInfoPanel v-if="envInfo" :env="envInfo" />
      <div class="content-row" v-if="fileList.length > 0">
        <FileList
          :files="fileList"
          :ignored="ignoredFiles"
          :force-download="forceDownloadFiles"
          @ignore="handleIgnore"
          @force="handleForce"
          @reset-file="handleResetFile"
        />
        <DownloadPanel
          :files="fileList"
          :headers="currentHeaders"
          :xml-filter="xmlFilter"
          :filter-pattern="currentFilterPattern"
          :ignored-files="ignoredFiles"
          :force-download-files="forceDownloadFiles"
          @update="handleDownloadUpdate"
          @reset="handleReset"
        />
      </div>
    </div>
  </div>
</template>

<style>
:root {
  --bg: #1a1a2e;
  --surface: #16213e;
  --surface-hover: #0f3460;
  --border: #2a2a4a;
  --text: #e0e0e0;
  --text-dim: #8892b0;
  --text-muted: #5a5a7a;
  --accent: #7c83ff;
  --accent2: #e94560;
  --title: #7c83ff;
  --input-bg: #0f3460;
  --btn-purple: #533483;
  --error-bg: #3d1f1f;
  --error-text: #ff6b6b;
}

:root.light {
  --bg: #f0f2f5;
  --surface: #ffffff;
  --surface-hover: #e8ecf1;
  --border: #d0d5dd;
  --text: #1a1a2e;
  --text-dim: #555;
  --text-muted: #888;
  --accent: #7986cb;
  --title: #9fa8da;
  --accent2: #d63851;
  --input-bg: #f7f8fa;
  --btn-purple: #6b4fa0;
  --error-bg: #fde8e8;
  --error-text: #d63851;
}

* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

html, body, #app {
  height: 100%;
  overflow: hidden;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  background: var(--bg);
  color: var(--text);
}

.app {
  height: 100%;
  display: flex;
  flex-direction: column;
  padding: 16px 20px;
  overflow: hidden;
  position: relative;
}

.theme-toggle {
  position: absolute;
  top: 14px;
  right: 18px;
  width: 32px;
  height: 32px;
  border: 1px solid var(--border);
  border-radius: 50%;
  background: var(--surface);
  color: var(--text);
  font-size: 1rem;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10;
}

.theme-toggle:hover {
  background: var(--surface-hover);
}

h1 {
  text-align: center;
  margin-bottom: 12px;
  color: var(--title);
  font-size: 1.4rem;
  flex-shrink: 0;
}

.error {
  background: var(--error-bg);
  color: var(--error-text);
  padding: 8px 12px;
  border-radius: 6px;
  margin: 8px 0;
  font-size: 0.85rem;
  flex-shrink: 0;
}

.main-area {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-height: 0;
}

.content-row {
  flex: 1;
  display: flex;
  gap: 10px;
  min-height: 0;
}
</style>
