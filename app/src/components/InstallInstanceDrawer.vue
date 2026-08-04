<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type {
  InstallProgress,
  LoaderCatalog,
  LoaderKind,
  MinecraftVersion,
} from "../types";
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
  install: [request: {
    name: string;
    versionId: string;
    loader: LoaderKind;
    loaderVersion: string | null;
  }];
  pause: [];
  resume: [];
  cancel: [];
}>();

const name = ref("");
const loader = ref<LoaderKind>("vanilla");
const loaderVersion = ref("");
const loaderCatalog = ref<LoaderCatalog | null>(null);
const loaderState = ref<"idle" | "loading" | "ready" | "error">("idle");
const loaderError = ref("");
const nameInput = ref<HTMLInputElement | null>(null);
const canSubmit = computed(
  () => Boolean(
    props.version
      && name.value.trim()
      && !props.installing
      && !props.paused
      && (loader.value === "vanilla" || (loaderState.value === "ready" && loaderVersion.value)),
  ),
);
const busy = computed(() => props.installing || props.paused);
const progressPercent = computed(() => {
  if (!props.progress?.totalFiles) return 0;
  return Math.min(100, Math.round((props.progress.completedFiles / props.progress.totalFiles) * 100));
});

function formatTransferRate(bytesPerSecond: number) {
  if (bytesPerSecond < 1024) return `${bytesPerSecond} B/s`;
  if (bytesPerSecond < 1024 * 1024) return `${(bytesPerSecond / 1024).toFixed(1)} KB/s`;
  return `${(bytesPerSecond / (1024 * 1024)).toFixed(1)} MB/s`;
}

const loaderLabels: Record<LoaderKind, string> = {
  vanilla: "原版 Vanilla",
  fabric: "Fabric",
  quilt: "Quilt",
  forge: "Forge",
  neoforge: "NeoForge",
};

async function loadLoaderCatalog() {
  loaderCatalog.value = null;
  loaderVersion.value = "";
  loaderError.value = "";
  if (!props.version || loader.value === "vanilla") {
    loaderState.value = "ready";
    return;
  }
  loaderState.value = "loading";
  try {
    const catalog = await invoke<LoaderCatalog>("list_loader_versions", {
      loader: loader.value,
      gameVersion: props.version.id,
    });
    loaderCatalog.value = catalog;
    loaderVersion.value =
      catalog.versions.find((version) => version.recommended)?.version
      ?? catalog.versions.find((version) => version.stable)?.version
      ?? catalog.versions[0]?.version
      ?? "";
    loaderState.value = "ready";
    if (!loaderVersion.value) loaderError.value = "该 Minecraft 版本没有可用的加载器版本";
  } catch (error) {
    loaderState.value = "error";
    loaderError.value = typeof error === "string" ? error : "无法读取加载器版本";
  }
}

watch(
  () => [props.open, props.version?.id] as const,
  ([open]) => {
    if (!open || !props.version) return;
    name.value = `Minecraft ${props.version.id}`;
    loader.value = "vanilla";
    loaderVersion.value = "";
    loaderCatalog.value = null;
    loaderState.value = "ready";
    loaderError.value = "";
    void nextTick(() => nameInput.value?.select());
  },
);

watch(loader, () => void loadLoaderCatalog());
</script>

<template>
  <Transition name="drawer">
    <div v-if="open && version" class="drawer-layer" role="presentation" @mousedown.self="emit('close')">
      <aside class="create-drawer" aria-labelledby="install-instance-title">
        <header class="drawer-header">
          <div>
            <div class="drawer-brand">NaCL · 官方版本</div>
            <h2 id="install-instance-title">安装 {{ version.id }}</h2>
          </div>
          <button class="icon-button" aria-label="收起安装面板" @click="emit('close')">
            <FlatIcon name="close" />
          </button>
        </header>
        <form
          class="create-form"
          @submit.prevent="emit('install', {
            name: name.trim(),
            versionId: version.id,
            loader,
            loaderVersion: loader === 'vanilla' ? null : loaderVersion,
          })"
        >
          <div class="form-intro">客户端、依赖库和资源文件校验完成后，NaCL 才会创建实例。</div>
          <label class="field">
            <span class="field-label">实例名称</span>
            <input ref="nameInput" v-model="name" maxlength="64" :disabled="busy" />
          </label>
          <label class="field">
            <span class="field-label">加载器</span>
            <select v-model="loader" :disabled="busy || loaderState === 'loading'">
              <option v-for="(label, kind) in loaderLabels" :key="kind" :value="kind">
                {{ label }}
              </option>
            </select>
          </label>
          <label v-if="loader !== 'vanilla'" class="field">
            <span class="field-label">加载器版本</span>
            <select v-model="loaderVersion" :disabled="busy || loaderState !== 'ready'">
              <option v-if="loaderState === 'loading'" value="">正在读取兼容版本…</option>
              <option
                v-for="candidate in loaderCatalog?.versions ?? []"
                :key="candidate.version"
                :value="candidate.version"
              >
                {{ candidate.version }}{{ candidate.recommended ? " · 推荐" : candidate.stable ? " · 稳定版" : " · 测试版" }}
              </option>
            </select>
            <span class="field-hint">只显示与 Minecraft {{ version.id }} 匹配的版本。</span>
          </label>
          <div v-if="loaderError" class="form-error" role="alert">{{ loaderError }}</div>
          <div class="drawer-summary">
            <div><span>版本</span><strong>{{ version.id }}</strong></div>
            <div><span>频道</span><strong>{{ version.type === "release" ? "正式版" : "快照版" }}</strong></div>
            <div><span>加载器</span><strong>{{ loaderLabels[loader] }}</strong></div>
            <div><span>默认内存</span><strong>{{ defaultMemoryMb / 1024 }} GB</strong></div>
          </div>
          <div v-if="busy" class="install-progress">
            <div class="install-progress-head">
              <span>{{ paused ? "已暂停" : progress?.stage === "assets" ? "下载资源" : progress?.stage === "loader" ? "安装加载器" : progress?.stage === "libraries" ? "下载依赖" : "准备安装" }}</span>
              <strong>{{ progressPercent }}%</strong>
            </div>
            <div class="progress-track"><span :style="{ width: `${progressPercent}%` }"></span></div>
            <div class="field-hint">
              {{ progress?.currentFile || "连接 Mojang 服务…" }} ·
              {{ paused ? "已暂停" : formatTransferRate(progress?.downloadSpeedBytesPerSecond ?? 0) }} ·
              {{ progress?.downloadEngine === "segmented" ? `原生分片 ${progress.activeConnections} 连接` : progress?.downloadEngine === "streaming" ? "流式下载" : "缓存命中" }}
            </div>
            <div class="install-controls">
              <button v-if="!paused" type="button" class="text-action" @click="emit('pause')">暂停</button>
              <button v-else type="button" class="text-action" @click="emit('resume')">继续</button>
              <button type="button" class="text-action danger-action" @click="emit('cancel')">取消安装</button>
            </div>
          </div>
          <div v-if="error" class="form-error" role="alert">{{ error }}</div>
          <div class="drawer-actions">
            <button type="button" class="secondary-button" @click="emit('close')">
              {{ busy ? "收起到安装队列" : "关闭" }}
            </button>
            <button type="submit" class="primary-button" :disabled="!canSubmit">
              {{ busy ? (paused ? "安装已暂停" : "正在安装…") : "开始安装" }}
            </button>
          </div>
        </form>
      </aside>
    </div>
  </Transition>
</template>
