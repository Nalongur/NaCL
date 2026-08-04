<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type {
  ContentKind,
  ContentProject,
  ContentSearchPage,
  ContentVersion,
  LauncherInstance,
  ManagedContent,
  WorldInfo,
} from "../types";
import FlatIcon from "./FlatIcon.vue";

const props = defineProps<{
  open: boolean;
  instance: LauncherInstance;
  initialKind: Exclude<ContentKind, "modpack">;
  worldName: string | null;
}>();

const emit = defineEmits<{
  close: [];
  installed: [items: ManagedContent[]];
}>();

const kind = ref<Exclude<ContentKind, "modpack">>("mod");
const query = ref("");
const results = ref<ContentProject[]>([]);
const totalHits = ref(0);
const state = ref<"idle" | "loading" | "ready" | "error">("idle");
const error = ref("");
const selectedProject = ref<ContentProject | null>(null);
const versions = ref<ContentVersion[]>([]);
const selectedVersionId = ref("");
const versionState = ref<"idle" | "loading" | "ready" | "error">("idle");
const installState = ref<"idle" | "installing">("idle");
const worlds = ref<WorldInfo[]>([]);
const selectedWorld = ref("");

const selectedVersion = computed(() =>
  versions.value.find((version) => version.id === selectedVersionId.value) ?? null,
);
const requiredDependencies = computed(() =>
  selectedVersion.value?.dependencies.filter((dependency) => dependency.dependencyType === "required") ?? [],
);
const targetWorld = computed(() =>
  props.worldName
  ?? selectedWorld.value
  ?? "",
);

const kindLabels: Record<Exclude<ContentKind, "modpack">, string> = {
  mod: "Mod",
  resourcepack: "资源包",
  shader: "光影包",
  datapack: "数据包",
};

async function search() {
  state.value = "loading";
  error.value = "";
  selectedProject.value = null;
  try {
    const page = await invoke<ContentSearchPage>("search_content", {
      request: {
        query: query.value.trim(),
        kind: kind.value,
        gameVersion: props.instance.gameVersion,
        loader: props.instance.loader.kind,
        offset: 0,
        limit: 30,
      },
    });
    results.value = page.hits;
    totalHits.value = page.totalHits;
    state.value = "ready";
  } catch (reason) {
    state.value = "error";
    error.value = typeof reason === "string" ? reason : "无法搜索社区内容";
  }
}

async function loadWorlds() {
  try {
    worlds.value = await invoke<WorldInfo[]>("list_instance_worlds", {
      instanceId: props.instance.id,
    });
    const preferred = props.worldName ?? selectedWorld.value;
    selectedWorld.value = worlds.value.some((world) => world.name === preferred)
      ? preferred
      : worlds.value[0]?.name ?? "";
  } catch {
    worlds.value = [];
    selectedWorld.value = "";
  }
}

async function selectKind(nextKind: Exclude<ContentKind, "modpack">) {
  if (kind.value === nextKind) return;
  kind.value = nextKind;
  query.value = "";
  if (nextKind === "datapack") await loadWorlds();
  await search();
}

async function selectProject(project: ContentProject) {
  selectedProject.value = project;
  versions.value = [];
  selectedVersionId.value = "";
  versionState.value = "loading";
  error.value = "";
  try {
    versions.value = await invoke<ContentVersion[]>("list_content_versions", {
      projectId: project.projectId,
      gameVersion: props.instance.gameVersion,
      loader: props.instance.loader.kind,
      kind: kind.value,
    });
    selectedVersionId.value =
      versions.value.find((version) => version.versionType === "release")?.id
      ?? versions.value[0]?.id
      ?? "";
    versionState.value = "ready";
  } catch (reason) {
    versionState.value = "error";
    error.value = typeof reason === "string" ? reason : "无法读取项目版本";
  }
}

async function installSelected() {
  if (!selectedVersion.value || !selectedProject.value) return;
  installState.value = "installing";
  error.value = "";
  try {
    const items = await invoke<ManagedContent[]>("install_content", {
      request: {
        instanceId: props.instance.id,
        versionId: selectedVersion.value.id,
        kind: kind.value,
        worldName: kind.value === "datapack" ? targetWorld.value : null,
      },
    });
    emit("installed", items);
    emit("close");
  } catch (reason) {
    error.value = typeof reason === "string" ? reason : "内容安装失败";
  } finally {
    installState.value = "idle";
  }
}

watch(
  () => [props.open, props.instance.id, props.initialKind] as const,
  async ([open]) => {
    if (!open) return;
    kind.value = props.initialKind;
    query.value = "";
    await loadWorlds();
    await search();
  },
);
</script>

<template>
  <Transition name="drawer">
    <div v-if="open" class="drawer-layer" role="presentation" @mousedown.self="emit('close')">
      <aside class="create-drawer content-browser" aria-labelledby="content-browser-title">
        <header class="drawer-header">
          <div>
            <div class="drawer-brand">{{ instance.name }} · {{ instance.gameVersion }} · {{ instance.loader.kind }}</div>
            <h2 id="content-browser-title">添加社区内容</h2>
          </div>
          <button class="icon-button" aria-label="关闭内容浏览" @click="emit('close')">
            <FlatIcon name="close" />
          </button>
        </header>

        <div class="content-kind-tabs">
          <button
            v-for="(label, value) in kindLabels"
            :key="value"
            type="button"
            :class="{ active: kind === value }"
            @click="selectKind(value)"
          >
            {{ label }}
          </button>
        </div>

        <label v-if="kind === 'datapack' && worlds.length" class="datapack-world-picker">
          <span>目标存档</span>
          <select v-model="selectedWorld">
            <option v-for="world in worlds" :key="world.name" :value="world.name">
              {{ world.name }} · {{ world.datapackCount }} 个数据包
            </option>
          </select>
        </label>
        <div v-else-if="kind === 'datapack'" class="datapack-world-empty">
          该实例还没有可用存档，请先进入游戏创建世界。
        </div>

        <form class="content-search" @submit.prevent="search">
          <input v-model="query" :placeholder="`搜索${kindLabels[kind]}`" />
          <button class="secondary-button" type="submit" :disabled="state === 'loading'">搜索</button>
        </form>

        <div v-if="error" class="form-error" role="alert">{{ error }}</div>
        <div v-if="state === 'loading'" class="data-empty">正在读取 Modrinth…</div>

        <div v-else-if="!selectedProject" class="content-results">
          <div class="section-note">{{ totalHits }} 个兼容项目</div>
          <button
            v-for="project in results"
            :key="project.projectId"
            type="button"
            class="content-result"
            @click="selectProject(project)"
          >
            <img v-if="project.iconUrl" :src="project.iconUrl" alt="" referrerpolicy="no-referrer" />
            <span>
              <strong>{{ project.title }}</strong>
              <small>{{ project.author }} · {{ project.description }}</small>
            </span>
            <span class="version-action">查看</span>
          </button>
          <div v-if="results.length === 0 && state === 'ready'" class="data-empty">没有匹配的兼容内容</div>
        </div>

        <div v-else class="content-detail">
          <button type="button" class="text-action" @click="selectedProject = null">返回搜索结果</button>
          <div class="content-detail-title">
            <img v-if="selectedProject.iconUrl" :src="selectedProject.iconUrl" alt="" referrerpolicy="no-referrer" />
            <div>
              <h3>{{ selectedProject.title }}</h3>
              <p>{{ selectedProject.description }}</p>
            </div>
          </div>
          <label class="field">
            <span class="field-label">兼容版本</span>
            <select v-model="selectedVersionId" :disabled="versionState !== 'ready' || installState === 'installing'">
              <option v-for="version in versions" :key="version.id" :value="version.id">
                {{ version.versionNumber }} · {{ version.versionType === "release" ? "稳定版" : version.versionType }}
              </option>
            </select>
          </label>
          <div v-if="selectedVersion" class="drawer-summary">
            <div><span>主文件</span><strong>{{ selectedVersion.files.find((file) => file.primary)?.filename ?? selectedVersion.files[0]?.filename }}</strong></div>
            <div><span>必需依赖</span><strong>{{ requiredDependencies.length }} 项</strong></div>
            <div><span>完整性</span><strong>SHA-512 / SHA-1</strong></div>
          </div>
          <div class="form-intro">NaCL 会先下载并校验主文件与必需依赖，全部通过后才写入实例。</div>
          <div class="drawer-actions">
            <button type="button" class="secondary-button" @click="selectedProject = null">取消</button>
            <button
              type="button"
              class="primary-button"
              :disabled="!selectedVersion || installState === 'installing' || (kind === 'datapack' && !targetWorld)"
              @click="installSelected"
            >
              {{ installState === "installing" ? "正在安装…" : `安装${requiredDependencies.length ? `及 ${requiredDependencies.length} 项依赖` : ""}` }}
            </button>
          </div>
        </div>
      </aside>
    </div>
  </Transition>
</template>

<style scoped>
.content-browser { width: min(720px, calc(100vw - 32px)); }
.content-kind-tabs { display: flex; gap: 8px; padding: 16px 24px 0; }
.content-kind-tabs button { border: 1px solid var(--line); background: transparent; color: var(--muted); padding: 8px 14px; border-radius: 10px; }
.content-kind-tabs button.active { color: var(--text); border-color: var(--accent); }
.datapack-world-picker { display: grid; grid-template-columns: auto minmax(220px, 1fr); align-items: center; gap: 12px; padding: 16px 24px 0; color: var(--muted); font-size: 12px; }
.datapack-world-picker select { height: 40px; border: 1px solid var(--line); border-radius: 9px; padding: 0 12px; background: var(--surface); color: var(--text); }
.datapack-world-empty { margin: 16px 24px 0; padding: 11px 13px; border-left: 2px solid var(--accent); background: var(--accent-soft); color: var(--muted); font-size: 12px; }
.content-search { display: grid; grid-template-columns: 1fr auto; gap: 10px; padding: 16px 24px; }
.content-results, .content-detail { display: grid; gap: 10px; padding: 0 24px 24px; overflow: auto; }
.content-result { display: grid; grid-template-columns: 44px 1fr auto; align-items: center; gap: 12px; border: 1px solid var(--line); background: transparent; color: var(--text); padding: 10px; border-radius: 12px; text-align: left; }
.content-result img, .content-detail-title img { width: 44px; height: 44px; border-radius: 10px; object-fit: cover; }
.content-result span:nth-child(2) { display: grid; gap: 4px; min-width: 0; }
.content-result small { color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.content-detail-title { display: flex; gap: 12px; align-items: center; }
.content-detail-title h3, .content-detail-title p { margin: 0; }
.content-detail-title p { color: var(--muted); margin-top: 4px; }
</style>
