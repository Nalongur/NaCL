<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { ContentUpdate, LauncherInstance, ManagedContent, WorldInfo } from "../types";
import ContentBrowserDrawer from "./ContentBrowserDrawer.vue";

const props = defineProps<{ instance: LauncherInstance }>();

const items = ref<ManagedContent[]>([]);
const state = ref<"loading" | "ready" | "error">("loading");
const error = ref("");
const browserOpen = ref(false);
const activeKind = ref<ManagedContent["kind"]>("mod");
const actionItemId = ref<string | null>(null);
const updates = ref<Record<string, ContentUpdate>>({});
const updateState = ref<"idle" | "checking">("idle");
const query = ref("");
const issuesOnly = ref(false);
const selectedIds = ref<string[]>([]);
const batchState = ref<"idle" | "enable" | "disable" | "update" | "remove">("idle");
const worlds = ref<WorldInfo[]>([]);
const selectedWorld = ref("");

const visibleItems = computed(() => {
  const keyword = query.value.trim().toLocaleLowerCase();
  return items.value.filter((item) => {
    if (
      item.kind !== activeKind.value
      || (activeKind.value === "datapack" && item.worldName !== selectedWorld.value)
      || (issuesOnly.value && item.diagnostics.length === 0)
    ) return false;
    if (!keyword) return true;
    return [item.name, item.fileName, item.versionNumber, ...(item.modMetadata?.modIds || [])]
      .filter(Boolean)
      .some((value) => String(value).toLocaleLowerCase().includes(keyword));
  });
});
const selectedVisibleCount = computed(() => visibleItems.value.filter((item) => selectedIds.value.includes(item.id)).length);
const kindLabels: Record<ManagedContent["kind"], string> = {
  mod: "Mod",
  resourcepack: "资源包",
  shader: "光影包",
  datapack: "数据包",
};

async function load() {
  state.value = "loading";
  error.value = "";
  try {
    items.value = await invoke<ManagedContent[]>("list_instance_content", {
      instanceId: props.instance.id,
    });
    worlds.value = await invoke<WorldInfo[]>("list_instance_worlds", { instanceId: props.instance.id });
    if (!worlds.value.some((world) => world.name === selectedWorld.value)) {
      selectedWorld.value = worlds.value[0]?.name ?? "";
    }
    state.value = "ready";
  } catch (reason) {
    state.value = "error";
    error.value = typeof reason === "string" ? reason : "无法读取实例内容";
  }
}

async function setEnabled(item: ManagedContent, enabled: boolean) {
  actionItemId.value = item.id;
  error.value = "";
  try {
    items.value = await invoke<ManagedContent[]>("set_content_enabled", {
      request: { instanceId: props.instance.id, itemId: item.id },
      enabled,
    });
  } catch (reason) {
    error.value = typeof reason === "string" ? reason : "无法切换内容状态";
  } finally {
    actionItemId.value = null;
  }
}

async function remove(item: ManagedContent) {
  if (!window.confirm(`移除“${item.name}”？文件会先移入实例的 NaCL 回收目录。`)) return;
  actionItemId.value = item.id;
  error.value = "";
  try {
    items.value = await invoke<ManagedContent[]>("remove_content", {
      request: { instanceId: props.instance.id, itemId: item.id },
    });
  } catch (reason) {
    error.value = typeof reason === "string" ? reason : "无法移除内容";
  } finally {
    actionItemId.value = null;
  }
}

async function importLocal() {
  const extension = activeKind.value === "mod" ? "jar" : "zip";
  const selected = await open({
    multiple: false,
    directory: false,
    title: `导入${kindLabels[activeKind.value]}`,
    filters: [{ name: kindLabels[activeKind.value], extensions: [extension] }],
  });
  if (!selected) return;
  error.value = "";
  actionItemId.value = "import";
  try {
    items.value = await invoke<ManagedContent[]>("import_local_content", {
      request: {
        instanceId: props.instance.id,
        sourcePath: selected,
        kind: activeKind.value,
        worldName: activeKind.value === "datapack" ? selectedWorld.value : null,
      },
    });
  } catch (reason) {
    error.value = typeof reason === "string" ? reason : "无法导入本地内容";
  } finally {
    actionItemId.value = null;
  }
}

async function checkUpdates() {
  updateState.value = "checking";
  error.value = "";
  try {
    const result = await invoke<ContentUpdate[]>("check_content_updates", { instanceId: props.instance.id });
    updates.value = Object.fromEntries(result.map((update) => [update.itemId, update]));
  } catch (reason) {
    error.value = typeof reason === "string" ? reason : "无法检查内容更新";
  } finally {
    updateState.value = "idle";
  }
}

async function installUpdate(item: ManagedContent) {
  const update = updates.value[item.id];
  if (!update) return;
  actionItemId.value = item.id;
  error.value = "";
  try {
    items.value = await invoke<ManagedContent[]>("install_content", {
      request: {
        instanceId: props.instance.id,
        versionId: update.versionId,
        kind: item.kind,
        worldName: item.worldName,
      },
    });
    delete updates.value[item.id];
  } catch (reason) {
    error.value = typeof reason === "string" ? reason : "内容更新失败";
  } finally {
    actionItemId.value = null;
  }
}

function toggleSelected(itemId: string) {
  selectedIds.value = selectedIds.value.includes(itemId)
    ? selectedIds.value.filter((id) => id !== itemId)
    : [...selectedIds.value, itemId];
}

function toggleAllVisible() {
  const visibleIds = visibleItems.value.map((item) => item.id);
  if (visibleIds.every((id) => selectedIds.value.includes(id))) {
    selectedIds.value = selectedIds.value.filter((id) => !visibleIds.includes(id));
  } else {
    selectedIds.value = [...new Set([...selectedIds.value, ...visibleIds])];
  }
}

async function runBatch(action: "enable" | "disable" | "update" | "remove") {
  if (selectedIds.value.length === 0) return;
  if (action === "remove" && !window.confirm(`移除选中的 ${selectedIds.value.length} 项？文件会先移入 NaCL 回收目录。`)) return;
  batchState.value = action;
  error.value = "";
  try {
    const request = { instanceId: props.instance.id, itemIds: selectedIds.value };
    if (action === "update") {
      items.value = await invoke<ManagedContent[]>("update_content_batch", { request });
      await checkUpdates();
    } else if (action === "remove") {
      items.value = await invoke<ManagedContent[]>("remove_content_batch", { request });
    } else {
      items.value = await invoke<ManagedContent[]>("set_content_enabled_batch", {
        request,
        enabled: action === "enable",
      });
    }
    selectedIds.value = [];
  } catch (reason) {
    error.value = typeof reason === "string" ? reason : "批量操作失败";
  } finally {
    batchState.value = "idle";
  }
}

watch(() => props.instance.id, () => { selectedIds.value = []; void load(); }, { immediate: true });
watch(activeKind, () => { selectedIds.value = []; issuesOnly.value = false; });
</script>

<template>
  <div class="instance-content-panel">
    <div class="content-toolbar">
      <div class="content-kind-tabs">
        <button
          v-for="(label, value) in kindLabels"
          :key="value"
          type="button"
          :class="{ active: activeKind === value }"
          @click="activeKind = value"
        >
          {{ label }}
        </button>
      </div>
      <div class="managed-content-actions">
        <button class="setting-action" type="button" :disabled="updateState === 'checking'" @click="checkUpdates">
          {{ updateState === "checking" ? "检查中…" : "检查更新" }}
        </button>
        <button class="setting-action" type="button" :disabled="actionItemId === 'import' || (activeKind === 'datapack' && !selectedWorld)" @click="importLocal">导入本地</button>
        <button class="setting-action add-content-action" type="button" :disabled="activeKind === 'datapack' && !selectedWorld" @click="browserOpen = true">添加内容</button>
      </div>
    </div>

    <div class="filter-toolbar">
      <select v-if="activeKind === 'datapack'" v-model="selectedWorld" class="world-select">
        <option v-for="world in worlds" :key="world.name" :value="world.name">{{ world.name }} · {{ world.datapackCount }} 个数据包</option>
      </select>
      <input v-model="query" class="content-search" type="search" placeholder="搜索名称、文件名或 Mod ID" />
      <label v-if="activeKind === 'mod'" class="issue-filter"><input v-model="issuesOnly" type="checkbox" /> 仅看问题</label>
      <button class="setting-action" type="button" :disabled="visibleItems.length === 0" @click="toggleAllVisible">
        {{ selectedVisibleCount === visibleItems.length && visibleItems.length ? "取消全选" : "全选当前" }}
      </button>
    </div>

    <div v-if="selectedIds.length" class="batch-toolbar">
      <span>已选择 {{ selectedIds.length }} 项</span>
      <button class="setting-action" type="button" :disabled="batchState !== 'idle'" @click="runBatch('enable')">批量启用</button>
      <button class="setting-action" type="button" :disabled="batchState !== 'idle'" @click="runBatch('disable')">批量禁用</button>
      <button class="setting-action" type="button" :disabled="batchState !== 'idle'" @click="runBatch('update')">批量更新</button>
      <button class="setting-action danger-action" type="button" :disabled="batchState !== 'idle'" @click="runBatch('remove')">批量移除</button>
    </div>

    <div v-if="error" class="inline-error">{{ error }}</div>
    <div v-if="state === 'loading'" class="data-empty">正在扫描实例内容…</div>
    <div v-else-if="visibleItems.length === 0" class="activity-empty">
      {{ activeKind === "datapack" && !selectedWorld ? "请先在游戏中创建一个存档。" : `当前实例还没有${kindLabels[activeKind]}。` }}
    </div>
    <div v-else class="managed-content-list">
      <article v-for="item in visibleItems" :key="item.id" class="managed-content-row">
        <input
          type="checkbox"
          :checked="selectedIds.includes(item.id)"
          :aria-label="`选择 ${item.name}`"
          @change="toggleSelected(item.id)"
        />
        <div class="content-detail">
          <div class="setting-name">{{ item.name }}</div>
          <div class="setting-description">
            {{ item.versionNumber || item.fileName }} · {{ item.managed ? "NaCL 管理" : "本地文件" }} ·
            {{ item.enabled ? "已启用" : "已禁用" }}
          </div>
          <div v-if="item.modMetadata" class="metadata-line">
            {{ item.modMetadata.format }} · {{ item.modMetadata.modIds.join(", ") }}
            <template v-if="item.modMetadata.authors.length"> · {{ item.modMetadata.authors.join(", ") }}</template>
          </div>
          <div v-if="item.modMetadata?.description" class="mod-description">{{ item.modMetadata.description }}</div>
          <div v-for="diagnostic in item.diagnostics" :key="diagnostic" class="diagnostic-line">{{ diagnostic }}</div>
        </div>
        <div class="managed-content-actions">
          <button
            v-if="updates[item.id]"
            class="setting-action"
            type="button"
            :disabled="actionItemId === item.id"
            @click="installUpdate(item)"
          >
            更新到 {{ updates[item.id].versionNumber }}
          </button>
          <button
            v-if="item.managed"
            class="setting-action"
            type="button"
            :disabled="actionItemId === item.id"
            @click="setEnabled(item, !item.enabled)"
          >
            {{ item.enabled ? "禁用" : "启用" }}
          </button>
          <button
            v-if="item.managed"
            class="setting-action danger-action"
            type="button"
            :disabled="actionItemId === item.id"
            @click="remove(item)"
          >
            移除
          </button>
        </div>
      </article>
    </div>

    <ContentBrowserDrawer
      :open="browserOpen"
      :instance="instance"
      :initial-kind="activeKind"
      :world-name="activeKind === 'datapack' ? selectedWorld : null"
      @close="browserOpen = false"
      @installed="items = $event"
    />
  </div>
</template>

<style scoped>
.instance-content-panel { display: grid; gap: 14px; margin-top: 16px; }
.content-toolbar { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.content-kind-tabs { display: flex; gap: 8px; }
.content-kind-tabs button { border: 0; background: transparent; color: var(--muted); padding: 8px 12px; border-radius: 9px; }
.content-kind-tabs button.active { color: var(--text); background: var(--surface-hover); }
.managed-content-list { display: grid; gap: 10px; }
.managed-content-row { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; gap: 14px; align-items: center; padding: 16px 18px; border: 1px solid var(--line); border-radius: 14px; }
.managed-content-actions { display: flex; gap: 8px; }
.add-content-action { border-color: var(--accent); background: var(--accent); color: var(--accent-ink); font-weight: 700; }
.add-content-action:hover:not(:disabled) { filter: brightness(1.05); }
.add-content-action:disabled { opacity: .46; }
.filter-toolbar, .batch-toolbar { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.content-search { min-width: 240px; flex: 1; border: 1px solid var(--line); border-radius: 9px; padding: 9px 11px; color: var(--text); background: transparent; }
.world-select { border: 1px solid var(--line); border-radius: 9px; padding: 9px 11px; color: var(--text); background: var(--surface); }
.issue-filter, .batch-toolbar span { color: var(--muted); font-size: 13px; }
.batch-toolbar { padding: 10px 12px; border: 1px solid var(--line); border-radius: 10px; }
.content-detail { min-width: 0; display: grid; gap: 4px; }
.metadata-line, .mod-description { color: var(--muted); font-size: 12px; overflow-wrap: anywhere; }
.mod-description { display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
.diagnostic-line { color: var(--danger, #d55b61); font-size: 12px; }
@media (max-width: 880px) { .managed-content-row { grid-template-columns: auto 1fr; } .managed-content-row > .managed-content-actions { grid-column: 2; flex-wrap: wrap; } }
</style>
