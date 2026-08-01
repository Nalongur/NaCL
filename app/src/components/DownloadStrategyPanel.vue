<script setup lang="ts">
import { ref, watch } from "vue";
import type { DownloadSettings } from "../types";

const props = defineProps<{
  settings: DownloadSettings;
  saving: boolean;
  error: string;
}>();
const emit = defineEmits<{
  update: [settings: DownloadSettings];
}>();
const draft = ref<DownloadSettings>({ ...props.settings });
watch(
  () => props.settings,
  () => {
    draft.value = { ...props.settings };
  },
  { deep: true, immediate: true },
);
function update() {
  emit("update", { ...draft.value });
}
</script>

<template>
  <div class="inline-settings-panel">
    <label class="setting-control">
      <span><strong>并发文件数</strong><small>机械硬盘或不稳定网络建议使用 2–4</small></span>
      <input v-model.number="draft.concurrentDownloads" type="number" min="1" max="16" :disabled="saving" @change="update" />
    </label>
    <label class="setting-control">
      <span><strong>大文件连接数</strong><small>NaCL 原生分片引擎；建议使用 4–8</small></span>
      <input v-model.number="draft.connectionsPerDownload" type="number" min="1" max="16" :disabled="saving" @change="update" />
    </label>
    <label class="setting-control">
      <span><strong>分片启用阈值</strong><small>MiB；更小的文件使用低开销流式下载</small></span>
      <input v-model.number="draft.segmentedDownloadThresholdMib" type="number" min="1" max="1024" :disabled="saving" @change="update" />
    </label>
    <label class="setting-control">
      <span><strong>失败重试</strong><small>单个文件下载失败后的自动重试次数</small></span>
      <input v-model.number="draft.retryCount" type="number" min="0" max="10" :disabled="saving" @change="update" />
    </label>
    <label class="setting-control">
      <span><strong>连接超时</strong><small>等待 Mojang 文件服务器响应的最长秒数</small></span>
      <input v-model.number="draft.connectionTimeoutSeconds" type="number" min="5" max="300" :disabled="saving" @change="update" />
    </label>
    <label class="setting-control">
      <span><strong>速度上限</strong><small>KiB/s；0 表示不限速</small></span>
      <input v-model.number="draft.speedLimitKibPerSecond" type="number" min="0" :disabled="saving" @change="update" />
    </label>
    <div class="setting-control">
      <span><strong>下载后校验</strong><small>始终使用 Mojang SHA-1 与文件大小校验</small></span>
      <input type="checkbox" checked disabled />
    </div>
    <div class="inline-save-state" :class="{ error }">
      {{ error || (saving ? "正在保存…" : "修改后自动保存") }}
    </div>
  </div>
</template>
