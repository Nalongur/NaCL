<script setup lang="ts">
import type { LogContent } from "../types";
import FlatIcon from "./FlatIcon.vue";

defineProps<{
  open: boolean;
  log: LogContent | null;
  loading: boolean;
  error: string;
}>();

const emit = defineEmits<{
  close: [];
  delete: [];
}>();
</script>

<template>
  <Transition name="drawer">
    <div v-if="open" class="drawer-layer" role="presentation" @mousedown.self="emit('close')">
      <aside class="create-drawer log-drawer" aria-labelledby="log-detail-title">
        <header class="drawer-header">
          <div>
            <div class="drawer-brand">NaCL · 日志</div>
            <h2 id="log-detail-title">{{ log?.name ?? "读取日志" }}</h2>
          </div>
          <div class="head-actions">
            <button v-if="log" class="setting-action danger-action" @click="emit('delete')">删除</button>
            <button class="icon-button" aria-label="关闭日志详情" @click="emit('close')">
              <FlatIcon name="close" />
            </button>
          </div>
        </header>
        <div class="log-content">
          <div v-if="loading" class="muted-state">正在读取…</div>
          <div v-else-if="error" class="form-error">{{ error }}</div>
          <template v-else-if="log">
            <div v-if="log.truncated" class="form-intro">日志较大，仅显示最后 512 KB。</div>
            <pre>{{ log.content || "日志为空。" }}</pre>
          </template>
        </div>
      </aside>
    </div>
  </Transition>
</template>
