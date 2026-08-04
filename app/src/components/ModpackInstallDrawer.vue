<script setup lang="ts">
import { ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { ContentProject, ContentSearchPage, ContentVersion } from "../types";
import FlatIcon from "./FlatIcon.vue";

const props = defineProps<{ open: boolean; installing: boolean; error: string }>();
const emit = defineEmits<{
  close: [];
  install: [request: { name: string; versionId: string | null; localPath: string | null }];
}>();

const query = ref("");
const projects = ref<ContentProject[]>([]);
const state = ref<"idle" | "loading" | "ready" | "error">("idle");
const localError = ref("");
const selectedProject = ref<ContentProject | null>(null);
const versions = ref<ContentVersion[]>([]);
const selectedVersionId = ref("");
const instanceName = ref("");

async function search() {
  state.value = "loading";
  localError.value = "";
  selectedProject.value = null;
  try {
    const page = await invoke<ContentSearchPage>("search_content", {
      request: { query: query.value.trim(), kind: "modpack", gameVersion: "", loader: null, offset: 0, limit: 30 },
    });
    projects.value = page.hits;
    state.value = "ready";
  } catch (reason) {
    state.value = "error";
    localError.value = typeof reason === "string" ? reason : "无法读取 Modrinth 整合包";
  }
}

async function selectProject(project: ContentProject) {
  selectedProject.value = project;
  instanceName.value = project.title;
  state.value = "loading";
  localError.value = "";
  try {
    const allVersions = await invoke<ContentVersion[]>("list_content_versions", {
      projectId: project.projectId,
      gameVersion: "",
      loader: null,
      kind: "modpack",
    });
    versions.value = allVersions.filter((version) => version.files.some((file) => file.filename.endsWith(".mrpack")));
    selectedVersionId.value = versions.value.find((version) => version.versionType === "release")?.id ?? versions.value[0]?.id ?? "";
    state.value = "ready";
  } catch (reason) {
    state.value = "error";
    localError.value = typeof reason === "string" ? reason : "无法读取整合包版本";
  }
}

async function importLocal() {
  localError.value = "";
  const selected = await openDialog({
    multiple: false,
    directory: false,
    title: "导入 Modrinth 整合包",
    filters: [{ name: "Modrinth 整合包", extensions: ["mrpack"] }],
  });
  if (!selected) return;
  const fileName = selected.split(/[\\/]/).pop()?.replace(/\.mrpack$/i, "") || "Modrinth 整合包";
  emit("install", { name: fileName, versionId: null, localPath: selected });
}

watch(() => props.open, (isOpen) => {
  if (!isOpen || state.value !== "idle") return;
  void search();
});
</script>

<template>
  <Transition name="drawer">
    <div v-if="open" class="drawer-layer" role="presentation" @mousedown.self="emit('close')">
      <aside class="create-drawer modpack-drawer" aria-labelledby="modpack-title">
        <header class="drawer-header">
          <div><div class="drawer-brand">Modrinth · .mrpack</div><h2 id="modpack-title">安装整合包</h2></div>
          <button class="icon-button" aria-label="关闭" :disabled="installing" @click="emit('close')"><FlatIcon name="close" /></button>
        </header>

        <div class="modpack-actions">
          <form class="content-search" @submit.prevent="search">
            <input v-model="query" placeholder="搜索整合包" />
            <button class="secondary-button" type="submit" :disabled="state === 'loading' || installing">搜索</button>
          </form>
          <button class="setting-action" type="button" :disabled="installing" @click="importLocal">导入本地 .mrpack</button>
        </div>
        <div v-if="error || localError" class="form-error" role="alert">{{ error || localError }}</div>
        <div v-if="state === 'loading'" class="data-empty">正在读取整合包信息…</div>

        <div v-else-if="!selectedProject" class="modpack-results">
          <button v-for="project in projects" :key="project.projectId" class="content-result" type="button" @click="selectProject(project)">
            <img v-if="project.iconUrl" :src="project.iconUrl" alt="" referrerpolicy="no-referrer" />
            <span><strong>{{ project.title }}</strong><small>{{ project.author }} · {{ project.description }}</small></span>
            <span class="version-action">选择</span>
          </button>
          <div v-if="state === 'ready' && projects.length === 0" class="data-empty">没有匹配的整合包</div>
        </div>

        <div v-else class="modpack-detail">
          <button class="text-action" type="button" :disabled="installing" @click="selectedProject = null">返回搜索结果</button>
          <div class="content-detail-title">
            <img v-if="selectedProject.iconUrl" :src="selectedProject.iconUrl" alt="" referrerpolicy="no-referrer" />
            <div><h3>{{ selectedProject.title }}</h3><p>{{ selectedProject.description }}</p></div>
          </div>
          <label class="field"><span class="field-label">实例名称</span><input v-model="instanceName" maxlength="64" /></label>
          <label class="field"><span class="field-label">整合包版本</span>
            <select v-model="selectedVersionId" :disabled="installing">
              <option v-for="version in versions" :key="version.id" :value="version.id">{{ version.versionNumber }} · {{ version.versionType }}</option>
            </select>
          </label>
          <div class="form-intro">将读取整合包声明的 Minecraft 与加载器版本，逐文件校验 SHA-512/SHA-1；任一步失败都会回滚新实例。</div>
          <div class="drawer-actions">
            <button class="secondary-button" type="button" :disabled="installing" @click="selectedProject = null">取消</button>
            <button class="primary-button" type="button" :disabled="installing || !selectedVersionId || !instanceName.trim()" @click="emit('install', { name: instanceName.trim(), versionId: selectedVersionId, localPath: null })">
              {{ installing ? "正在安装并校验…" : "创建整合包实例" }}
            </button>
          </div>
        </div>
      </aside>
    </div>
  </Transition>
</template>

<style scoped>
.modpack-drawer { width: min(720px, calc(100vw - 32px)); }
.modpack-actions { display: grid; grid-template-columns: 1fr auto; align-items: center; gap: 10px; padding: 16px 24px; }
.content-search { display: grid; grid-template-columns: 1fr auto; gap: 10px; }
.modpack-results, .modpack-detail { display: grid; gap: 10px; padding: 0 24px 24px; overflow: auto; }
.content-result { display: grid; grid-template-columns: 44px 1fr auto; align-items: center; gap: 12px; border: 1px solid var(--line); background: transparent; color: var(--text); padding: 10px; border-radius: 12px; text-align: left; }
.content-result img, .content-detail-title img { width: 44px; height: 44px; border-radius: 10px; object-fit: cover; }
.content-result span:nth-child(2) { display: grid; gap: 4px; min-width: 0; }
.content-result small { color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.content-detail-title { display: flex; gap: 12px; align-items: center; }
.content-detail-title h3, .content-detail-title p { margin: 0; }
.content-detail-title p { color: var(--muted); margin-top: 4px; }
</style>
