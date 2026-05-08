<script setup lang="ts">
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import InputPanel from "./components/InputPanel.vue";
import EnvInfoPanel from "./components/EnvInfoPanel.vue";
import FileList from "./components/FileList.vue";
import DownloadPanel from "./components/DownloadPanel.vue";
import type { FileItem, EnvInfo, FetchResult } from "./types";

const envInfo = ref<EnvInfo | null>(null);
const fileList = ref<FileItem[]>([]);
const loading = ref(false);
const error = ref("");
const currentHeaders = ref<Record<string, string>>({});

async function handleParse(jsonUrl: string, headers: Record<string, string>, filterPattern: string) {
  loading.value = true;
  error.value = "";
  currentHeaders.value = headers;
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

function handleReset() {
  envInfo.value = null;
  fileList.value = [];
  error.value = "";
  currentHeaders.value = {};
}
</script>

<template>
  <div class="app">
    <h1>Cosmic Resource Downloader</h1>
    <InputPanel @parse="handleParse" :loading="loading" />
    <div v-if="error" class="error">{{ error }}</div>
    <div class="main-area">
      <EnvInfoPanel v-if="envInfo" :env="envInfo" />
      <div class="content-row" v-if="fileList.length > 0">
        <FileList :files="fileList" />
        <DownloadPanel
          :files="fileList"
          :headers="currentHeaders"
          @update="handleDownloadUpdate"
          @reset="handleReset"
        />
      </div>
    </div>
  </div>
</template>

<style>
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
  background: #1a1a2e;
  color: #e0e0e0;
}

.app {
  height: 100%;
  display: flex;
  flex-direction: column;
  padding: 16px 20px;
  overflow: hidden;
}

h1 {
  text-align: center;
  margin-bottom: 12px;
  color: #7c83ff;
  font-size: 1.4rem;
  flex-shrink: 0;
}

.error {
  background: #3d1f1f;
  color: #ff6b6b;
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
