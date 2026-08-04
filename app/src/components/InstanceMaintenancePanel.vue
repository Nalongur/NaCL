<script setup lang="ts">
import { ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import type {
  CrashAnalysis,
  ExportInstanceReport,
  InstanceFileReport,
  LauncherInstance,
  RepairInstanceReport,
} from "../types";

const props = defineProps<{ instance: LauncherInstance }>();

const scanState = ref<"idle" | "running" | "repairing">("idle");
const crashState = ref<"idle" | "running">("idle");
const exportState = ref<"idle" | "running">("idle");
const fileReport = ref<InstanceFileReport | null>(null);
const repairReport = ref<RepairInstanceReport | null>(null);
const crashReport = ref<CrashAnalysis | null>(null);
const exportReport = ref<ExportInstanceReport | null>(null);
const error = ref("");

function message(reason: unknown, fallback: string) {
  return typeof reason === "string" ? reason : fallback;
}

async function scanFiles() {
  scanState.value = "running";
  error.value = "";
  repairReport.value = null;
  try {
    fileReport.value = await invoke<InstanceFileReport>("scan_instance_files", {
      instanceId: props.instance.id,
    });
  } catch (reason) {
    error.value = message(reason, "无法校验实例文件");
  } finally {
    scanState.value = "idle";
  }
}

async function repairFiles() {
  if (!window.confirm("将重新校验并补全客户端、加载器、依赖库和资源文件。继续吗？")) return;
  scanState.value = "repairing";
  error.value = "";
  try {
    repairReport.value = await invoke<RepairInstanceReport>("repair_instance", {
      instanceId: props.instance.id,
    });
    fileReport.value = repairReport.value.after;
  } catch (reason) {
    error.value = message(reason, "无法修复实例文件");
  } finally {
    scanState.value = "idle";
  }
}

async function analyzeCrash() {
  crashState.value = "running";
  error.value = "";
  try {
    crashReport.value = await invoke<CrashAnalysis>("analyze_instance_crash", {
      instanceId: props.instance.id,
    });
  } catch (reason) {
    error.value = message(reason, "无法分析游戏日志");
  } finally {
    crashState.value = "idle";
  }
}

async function exportFiles() {
  const destination = await save({
    title: "导出 NaCL 实例",
    defaultPath: `${props.instance.name}.zip`,
    filters: [{ name: "ZIP 归档", extensions: ["zip"] }],
  });
  if (!destination) return;
  exportState.value = "running";
  error.value = "";
  try {
    exportReport.value = await invoke<ExportInstanceReport>("export_instance", {
      instanceId: props.instance.id,
      destination,
    });
  } catch (reason) {
    error.value = message(reason, "无法导出实例");
  } finally {
    exportState.value = "idle";
  }
}

async function openInstanceDirectory() {
  error.value = "";
  try {
    await invoke("open_directory", { target: "instance", instanceId: props.instance.id });
  } catch (reason) {
    error.value = message(reason, "无法打开实例目录");
  }
}

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`;
  return `${(value / 1024 / 1024).toFixed(1)} MiB`;
}

watch(
  () => props.instance.id,
  () => {
    fileReport.value = null;
    repairReport.value = null;
    crashReport.value = null;
    exportReport.value = null;
    error.value = "";
  },
);
</script>

<template>
  <div class="maintenance-panel">
    <div v-if="error" class="inline-error">{{ error }}</div>

    <section class="maintenance-card">
      <div class="card-head">
        <div>
          <div class="setting-name">文件校验与补全</div>
          <div class="setting-description">校验客户端、加载器、依赖库、原生库和资源对象的大小与 SHA-1。</div>
        </div>
        <div class="card-actions">
          <button class="setting-action" type="button" :disabled="scanState !== 'idle'" @click="scanFiles">
            {{ scanState === "running" ? "校验中…" : "开始校验" }}
          </button>
          <button class="setting-action" type="button" :disabled="scanState !== 'idle'" @click="repairFiles">
            {{ scanState === "repairing" ? "补全中…" : "补全文件" }}
          </button>
        </div>
      </div>
      <div v-if="fileReport" class="report-body">
        <div class="metric-row">
          <span>扫描 {{ fileReport.scannedFiles }}</span>
          <span class="good">正常 {{ fileReport.validFiles }}</span>
          <span :class="{ bad: fileReport.missingFiles > 0 }">缺失 {{ fileReport.missingFiles }}</span>
          <span :class="{ bad: fileReport.corruptedFiles > 0 }">损坏 {{ fileReport.corruptedFiles }}</span>
        </div>
        <div v-if="repairReport" class="result-note">本次修复 {{ repairReport.repairedFiles }} 个问题。</div>
        <div v-if="fileReport.issues.length" class="issue-list">
          <div v-for="issue in fileReport.issues.slice(0, 30)" :key="`${issue.path}-${issue.state}`" class="issue-row">
            <div><strong>{{ issue.detail }}</strong><span>{{ issue.category }} · {{ issue.state }}</span></div>
            <code>{{ issue.path }}</code>
          </div>
          <div v-if="fileReport.issues.length > 30" class="result-note">另有 {{ fileReport.issues.length - 30 }} 个问题未展开。</div>
        </div>
        <div v-else class="result-note good">全部受管文件均通过校验。</div>
      </div>
    </section>

    <section class="maintenance-card">
      <div class="card-head">
        <div>
          <div class="setting-name">崩溃与启动诊断</div>
          <div class="setting-description">读取最近的 crash report 与游戏日志，本地识别内存、依赖、Mixin、Java 和图形问题。</div>
        </div>
        <button class="setting-action" type="button" :disabled="crashState === 'running'" @click="analyzeCrash">
          {{ crashState === "running" ? "分析中…" : "分析最近日志" }}
        </button>
      </div>
      <div v-if="crashReport" class="report-body">
        <div class="analysis-summary" :class="crashReport.status">{{ crashReport.summary }}</div>
        <article v-for="finding in crashReport.findings" :key="finding.code" class="finding-card">
          <div class="setting-name">{{ finding.title }}</div>
          <div class="setting-description">{{ finding.description }}</div>
          <ul><li v-for="action in finding.actions" :key="action">{{ action }}</li></ul>
          <details v-if="finding.evidence.length"><summary>查看日志证据</summary><code v-for="line in finding.evidence" :key="line">{{ line }}</code></details>
        </article>
      </div>
    </section>

    <section class="maintenance-card">
      <div class="card-head">
        <div>
          <div class="setting-name">实例目录与导出</div>
          <div class="setting-description">导出配置、存档、截图、模组和资源；自动排除日志、缓存、原生库、备份与回收目录。</div>
        </div>
        <div class="card-actions">
          <button class="setting-action" type="button" @click="openInstanceDirectory">打开目录</button>
          <button class="setting-action" type="button" :disabled="exportState === 'running'" @click="exportFiles">
            {{ exportState === "running" ? "导出中…" : "导出 ZIP" }}
          </button>
        </div>
      </div>
      <div v-if="exportReport" class="result-note good">
        已导出 {{ exportReport.files }} 个文件（{{ formatBytes(exportReport.bytes) }}）到 {{ exportReport.destination }}
      </div>
    </section>
  </div>
</template>

<style scoped>
.maintenance-panel { display: grid; gap: 12px; }
.maintenance-card { display: grid; gap: 14px; padding: 18px; border: 1px solid var(--line); border-radius: 14px; }
.card-head { display: flex; align-items: center; justify-content: space-between; gap: 18px; }
.card-actions, .metric-row { display: flex; gap: 8px; flex-wrap: wrap; }
.report-body { display: grid; gap: 10px; }
.metric-row span { padding: 6px 9px; border: 1px solid var(--line); border-radius: 8px; color: var(--muted); }
.good { color: var(--success, #35a26b) !important; }
.bad { color: var(--danger, #d55b61) !important; }
.result-note, .analysis-summary { color: var(--muted); font-size: 13px; }
.analysis-summary.error { color: var(--danger, #d55b61); }
.issue-list { display: grid; gap: 7px; max-height: 280px; overflow: auto; }
.issue-row { display: grid; grid-template-columns: minmax(180px, .8fr) minmax(260px, 1.2fr); gap: 12px; padding: 9px 10px; background: var(--surface-hover); border-radius: 9px; }
.issue-row div { display: grid; gap: 2px; }
.issue-row span { color: var(--muted); font-size: 12px; }
code { display: block; overflow-wrap: anywhere; color: var(--muted); font: 12px/1.5 ui-monospace, SFMono-Regular, Consolas, monospace; }
.finding-card { display: grid; gap: 7px; padding: 12px; background: var(--surface-hover); border-radius: 10px; }
.finding-card ul { margin: 0; padding-left: 20px; color: var(--muted); font-size: 13px; }
.finding-card details { display: grid; gap: 5px; }
.finding-card summary { cursor: pointer; color: var(--muted); font-size: 13px; }
@media (max-width: 820px) { .card-head { align-items: flex-start; flex-direction: column; } .issue-row { grid-template-columns: 1fr; } }
</style>
