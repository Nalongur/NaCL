<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import type { InstallProgress, MinecraftVersion } from "../types";
import FlatIcon from "./FlatIcon.vue";

const props = defineProps<{
  open: boolean;
  version: MinecraftVersion | null;
  installing: boolean;
  paused: boolean;
  progress: InstallProgress | null;
  error: string;
  defaultMemoryMb: number;
}>();

const emit = defineEmits<{
  close: [];
  install: [request: { name: string; versionId: string }];
  pause: [];
  resume: [];
  cancel: [];
}>();

const name = ref("");
const nameInput = ref<HTMLInputElement | null>(null);
const canSubmit = computed(
  () => Boolean(props.version && name.value.trim() && !props.installing && !props.paused),
);
const busy = computed(() => props.installing || props.paused);
const progressPercent = computed(() => {
  if (!props.progress?.totalFiles) return 0;
  return Math.min(100, Math.round((props.progress.completedFiles / props.progress.totalFiles) * 100));
});

watch(
  () => [props.open, props.version?.id] as const,
  ([open]) => {
    if (!open || !props.version) return;
    name.value = `Minecraft ${props.version.id}`;
    void nextTick(() => nameInput.value?.select());
  },
);
</script>

<template>
  <Transition name="drawer">
    <div v-if="open && version" class="drawer-layer" role="presentation" @mousedown.self="!busy && emit('close')">
      <aside class="create-drawer" aria-labelledby="install-instance-title">
        <header class="drawer-header">
          <div>
            <div class="drawer-brand">NaCL · 官方版本</div>
            <h2 id="install-instance-title">安装 {{ version.id }}</h2>
          </div>
          <button class="icon-button" aria-label="关闭安装面板" :disabled="busy" @click="emit('close')">
            <FlatIcon name="close" />
          </button>
        </header>
        <form class="create-form" @submit.prevent="emit('install', { name: name.trim(), versionId: version.id })">
          <div class="form-intro">客户端、依赖库和资源文件校验完成后，NaCL 才会创建实例。</div>
          <label class="field">
            <span class="field-label">实例名称</span>
            <input ref="nameInput" v-model="name" maxlength="64" :disabled="busy" />
          </label>
          <div class="drawer-summary">
            <div><span>版本</span><strong>{{ version.id }}</strong></div>
            <div><span>频道</span><strong>{{ version.type === "release" ? "正式版" : "快照版" }}</strong></div>
            <div><span>默认内存</span><strong>{{ defaultMemoryMb / 1024 }} GB</strong></div>
          </div>
          <div v-if="busy" class="install-progress">
            <div class="install-progress-head">
              <span>{{ paused ? "已暂停" : progress?.stage === "assets" ? "下载资源" : progress?.stage === "libraries" ? "下载依赖" : "准备安装" }}</span>
              <strong>{{ progressPercent }}%</strong>
            </div>
            <div class="progress-track"><span :style="{ width: `${progressPercent}%` }"></span></div>
            <div class="field-hint">{{ progress?.currentFile || "连接 Mojang 服务…" }}</div>
            <div class="install-controls">
              <button v-if="!paused" type="button" class="text-action" @click="emit('pause')">暂停</button>
              <button v-else type="button" class="text-action" @click="emit('resume')">继续</button>
              <button type="button" class="text-action danger-action" @click="emit('cancel')">取消安装</button>
            </div>
          </div>
          <div v-if="error" class="form-error" role="alert">{{ error }}</div>
          <div class="drawer-actions">
            <button type="button" class="secondary-button" :disabled="busy" @click="emit('close')">关闭</button>
            <button type="submit" class="primary-button" :disabled="!canSubmit">
              {{ busy ? (paused ? "安装已暂停" : "正在安装…") : "开始安装" }}
            </button>
          </div>
        </form>
      </aside>
    </div>
  </Transition>
</template>
