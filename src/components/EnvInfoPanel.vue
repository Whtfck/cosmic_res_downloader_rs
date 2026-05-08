<script setup lang="ts">
import type { EnvInfo } from "../types";

defineProps<{ env: EnvInfo }>();
</script>

<template>
  <div class="env-info">
    <div class="row1">
      <span><b>环境:</b> {{ env.number }}</span>
      <span><b>名称:</b> {{ env.name }}</span>
    </div>
    <div class="row2">
      <b>ZK:</b> <span class="zk-value">{{ env.zk_address }}</span>
    </div>
  </div>
</template>

<style scoped>
.env-info {
  background: #16213e;
  padding: 8px 12px;
  border-radius: 6px;
  font-size: 0.8rem;
  flex-shrink: 0;
}

.row1 {
  display: flex;
  gap: 20px;
  margin-bottom: 4px;
}

.row1 b, .row2 b {
  color: #8892b0;
  font-weight: 500;
}

.row2 {
  display: flex;
  gap: 6px;
  align-items: baseline;
}

.zk-value {
  font-family: "Cascadia Code", "Fira Code", monospace;
  font-size: 0.75rem;
  user-select: all;
  -webkit-user-select: all;
  word-break: break-all;
  line-height: 1.4;
}
</style>
