<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import DownloadStrategyPanel from "./components/DownloadStrategyPanel.vue";
import FlatIcon, { type FlatIconName } from "./components/FlatIcon.vue";
import InstallInstanceDrawer from "./components/InstallInstanceDrawer.vue";
import InstanceSettingsDrawer from "./components/InstanceSettingsDrawer.vue";
import LogDetailDrawer from "./components/LogDetailDrawer.vue";
import SettingsSectionPanel from "./components/SettingsSectionPanel.vue";
import brandIcon from "./assets/brand/nacl-icon.svg";
import type {
  DiagnosticReport,
  DownloadSettings,
  InstallProgress,
  InstanceTab,
  JavaRuntime,
  LauncherInstance,
  LauncherSettings,
  LogContent,
  LogFile,
  MinecraftVersion,
  MemoryReport,
  Page,
  SettingsSection,
  StorageReport,
  Theme,
  VersionCatalog,
} from "./types";

type ResizeDirection =
  | "East"
  | "North"
  | "NorthEast"
  | "NorthWest"
  | "South"
  | "SouthEast"
  | "SouthWest"
  | "West";

type DownloadSection = "versions" | "queue" | "strategy";
type LogSection = "launcher" | "game";
type HelpSection = "diagnostics" | "directories" | "about";

interface SettingRow {
  name: string;
  description: string;
  value: string;
  action: string;
}

interface AppBootstrap {
  paths: {
    roamingRoot: string;
    localRoot: string;
    logsDir: string;
    downloadsDir: string;
  };
  settings: LauncherSettings;
  downloadSettings: DownloadSettings;
}

const currentPage = ref<Page>("home");
const currentTab = ref<InstanceTab>("overview");
const theme = ref<Theme>("dark");
const javaRuntimes = ref<JavaRuntime[]>([]);
const javaDetectionState = ref<"loading" | "ready" | "unavailable">("loading");
const javaDownloadState = ref<"idle" | "downloading">("idle");
const javaError = ref("");
const memoryReport = ref<MemoryReport | null>(null);
const memoryState = ref<"loading" | "ready" | "unavailable">("loading");
const settingsBackendAvailable = ref(false);
const launcherSettings = ref<LauncherSettings>({
  schemaVersion: 1,
  theme: "dark",
  selectedInstanceId: null,
  defaultPage: "home",
  rememberLastInstance: true,
  closeBehavior: "exit",
  checkUpdates: true,
  notifications: true,
  animationsEnabled: true,
  animationSpeed: "normal",
  interfaceDensity: "comfortable",
  useSmileyHeadings: true,
  autoDetectJava: true,
  preferredJavaPath: null,
  manageRuntimes: true,
  compatibilityWarnings: true,
  defaultMemoryMb: 4096,
  defaultWindowWidth: 1280,
  defaultWindowHeight: 720,
  logLevel: "info",
  logRetentionDays: 14,
  autoDiagnostics: true,
});
const downloadSettings = ref<DownloadSettings>({
  schemaVersion: 1,
  concurrentDownloads: 4,
  connectionsPerDownload: 4,
  retryCount: 3,
  connectionTimeoutSeconds: 30,
  speedLimitKibPerSecond: 0,
  showSnapshots: false,
  verifyAfterDownload: true,
});
const appPaths = ref<AppBootstrap["paths"] | null>(null);
const instances = ref<LauncherInstance[]>([]);
const selectedInstanceId = ref<string | null>(null);
const instancesState = ref<"loading" | "ready" | "unavailable">("loading");
const installPanelOpen = ref(false);
const installState = ref<"idle" | "installing" | "paused">("idle");
const installCancelledByUser = ref(false);
const installError = ref("");
const installProgress = ref<InstallProgress | null>(null);
const selectedVersion = ref<MinecraftVersion | null>(null);
const versionCatalog = ref<VersionCatalog | null>(null);
const versionState = ref<"idle" | "loading" | "ready" | "unavailable">("idle");
const versionError = ref("");
const versionSearch = ref("");
const downloadSaveState = ref<"idle" | "saving">("idle");
const downloadSaveError = ref("");
const downloadSection = ref<DownloadSection>("versions");
const instanceEditorOpen = ref(false);
const instanceSaveState = ref<"idle" | "saving">("idle");
const instanceSaveError = ref("");
const settingsSection = ref<SettingsSection>("general");
const settingsSaveState = ref<"idle" | "saving">("idle");
const settingsSaveError = ref("");
const logs = ref<LogFile[]>([]);
const logsState = ref<"loading" | "ready" | "unavailable">("loading");
const logPanelOpen = ref(false);
const activeLog = ref<LogContent | null>(null);
const logReadState = ref<"idle" | "loading">("idle");
const logReadError = ref("");
const diagnostic = ref<DiagnosticReport | null>(null);
const diagnosticState = ref<"loading" | "ready" | "unavailable">("loading");
const copyState = ref<"idle" | "copied" | "error">("idle");
const storage = ref<StorageReport | null>(null);
const maintenanceState = ref<"idle" | "working" | "done" | "error">("idle");
const logSearch = ref("");
const logSection = ref<LogSection>("launcher");
const helpSection = ref<HelpSection>("diagnostics");
let unlistenInstallProgress: UnlistenFn | null = null;

const railItems: Array<{ page: Page; icon: FlatIconName; label: string }> = [
  { page: "home", icon: "home", label: "首页" },
  { page: "instances", icon: "instances", label: "实例" },
  { page: "downloads", icon: "download", label: "下载" },
  { page: "logs", icon: "file", label: "日志" },
];

const tabs: Array<{ id: InstanceTab; label: string }> = [
  { id: "overview", label: "概览" },
  { id: "version", label: "游戏版本" },
  { id: "runtime", label: "Java 与 JVM" },
  { id: "display", label: "显示" },
  { id: "files", label: "文件" },
  { id: "advanced", label: "高级" },
];

const resizeHandles: Array<{
  direction: ResizeDirection;
  position: string;
}> = [
  { direction: "North", position: "north" },
  { direction: "NorthEast", position: "north-east" },
  { direction: "East", position: "east" },
  { direction: "SouthEast", position: "south-east" },
  { direction: "South", position: "south" },
  { direction: "SouthWest", position: "south-west" },
  { direction: "West", position: "west" },
  { direction: "NorthWest", position: "north-west" },
];

const settings: Record<InstanceTab, SettingRow[]> = {
  overview: [
    { name: "游戏版本", description: "该实例使用的 Minecraft 版本", value: "Minecraft 1.21 · 正式版", action: "检查" },
    { name: "Java 运行环境", description: "启动前自动检查兼容性", value: "Java 21 · 64 位", action: "更改" },
    { name: "内存分配", description: "仅应用于当前实例", value: "最小 1 GB · 最大 4 GB", action: "调整" },
    { name: "文件状态", description: "客户端、资源和依赖库", value: "完整 · 上次检查于今天", action: "重新检查" },
  ],
  version: [
    { name: "Minecraft 版本", description: "切换版本前会检查存档兼容风险", value: "1.21", action: "选择版本" },
    { name: "版本频道", description: "控制版本选择器显示范围", value: "正式版", action: "更改" },
  ],
  runtime: [
    { name: "Java 路径", description: "自动管理或使用自定义运行环境", value: "自动选择 · Java 21", action: "浏览" },
    { name: "内存", description: "当前实例的 JVM 内存上限", value: "4 GB", action: "调整" },
    { name: "JVM 参数", description: "高级用户可覆盖默认参数", value: "使用推荐参数", action: "编辑" },
  ],
  display: [
    { name: "启动模式", description: "窗口、最大化或全屏", value: "窗口模式", action: "更改" },
    { name: "窗口尺寸", description: "Minecraft 客户端初始尺寸", value: "1280 × 720", action: "调整" },
  ],
  files: [
    { name: "实例目录", description: "存档、截图、资源包与配置", value: "instances\\survival", action: "打开目录" },
    { name: "文件占用", description: "不包含共享资源和依赖库", value: "386 MB", action: "查看" },
  ],
  advanced: [
    { name: "游戏参数", description: "在默认参数之后追加", value: "未设置", action: "编辑" },
    { name: "调试模式", description: "记录更详细的启动信息", value: "关闭", action: "启用" },
  ],
};

const primaryJava = computed(
  () =>
    javaRuntimes.value.find(
      (runtime) => runtime.path === launcherSettings.value.preferredJavaPath,
    ) ??
    javaRuntimes.value[0],
);
const selectedInstance = computed(
  () =>
    instances.value.find((instance) => instance.id === selectedInstanceId.value) ??
    instances.value[0] ??
    null,
);
const visibleVersions = computed(() => {
  const query = versionSearch.value.trim().toLowerCase();
  return (versionCatalog.value?.versions ?? [])
    .filter((version) =>
      downloadSettings.value.showSnapshots
        ? version.type === "release" || version.type === "snapshot"
        : version.type === "release",
    )
    .filter((version) => !query || version.id.toLowerCase().includes(query))
    .slice(0, 80);
});
const visibleLogs = computed(() => {
  const query = logSearch.value.trim().toLowerCase();
  return logs.value.filter((log) => !query || log.name.toLowerCase().includes(query));
});
const settingsSectionTitle = computed(
  () =>
    ({
      general: "常规",
      appearance: "外观",
      java: "Java",
      defaults: "新实例默认值",
      storage: "存储与维护",
      logs: "日志与诊断",
    })[settingsSection.value],
);
const installedInstances = computed(() =>
  [...instances.value]
    .filter((instance) => instance.installation.state === "ready")
    .sort(
      (left, right) =>
        (right.installation.installedEpochMs ?? 0) -
        (left.installation.installedEpochMs ?? 0),
    ),
);
const installProgressPercent = computed(() => {
  if (!installProgress.value?.totalFiles) return 0;
  return Math.min(
    100,
    Math.round(
      (installProgress.value.completedFiles /
        installProgress.value.totalFiles) *
        100,
    ),
  );
});
const pageContext = computed(() => {
  const contexts: Record<
    Page,
    {
      title: string;
      label: string;
      items: Array<{ label: string; section: string }>;
    }
  > = {
    home: { title: "快速启动", label: "最近实例", items: [] },
    instances: { title: "实例", label: "全部实例", items: [] },
    downloads: {
      title: "下载",
      label: "Minecraft",
      items: [
        { label: "版本浏览", section: "versions" },
        { label: "安装队列", section: "queue" },
        { label: "下载策略", section: "strategy" },
      ],
    },
    logs: {
      title: "日志",
      label: "本地记录",
      items: [
        { label: "启动器日志", section: "launcher" },
        { label: "游戏日志", section: "game" },
      ],
    },
    settings: {
      title: "全局设置",
      label: "NaCL",
      items: [
        { label: "常规", section: "general" },
        { label: "外观", section: "appearance" },
        { label: "Java", section: "java" },
        { label: "新实例默认值", section: "defaults" },
        { label: "存储与维护", section: "storage" },
        { label: "日志与诊断", section: "logs" },
      ],
    },
    help: {
      title: "帮助",
      label: "支持",
      items: [
        { label: "环境诊断", section: "diagnostics" },
        { label: "数据目录", section: "directories" },
        { label: "关于 NaCL", section: "about" },
      ],
    },
  };
  return contexts[currentPage.value];
});

const javaSummary = computed(() => {
  if (javaDetectionState.value === "loading") return "正在检测 Java…";
  if (javaDetectionState.value === "unavailable") return "桌面端检测";
  if (!primaryJava.value) return "未检测到 Java";

  const version = primaryJava.value.version
    ? `Java ${primaryJava.value.version}`
    : "Java";
  const architecture = formatArchitecture(primaryJava.value.architecture);
  return architecture ? `${version} · ${architecture}` : version;
});

const visibleSettings = computed(() =>
  settings[currentTab.value].map((row) => {
    const instance = selectedInstance.value;
    if (row.name === "游戏版本" || row.name === "Minecraft 版本") {
      return {
        ...row,
        value: instance ? `Minecraft ${instance.gameVersion}` : "未选择实例",
      };
    }
    if (row.name === "Java 运行环境") {
      return { ...row, value: javaSummary.value };
    }
    if (row.name === "Java 路径") {
      return {
        ...row,
        value:
          instance?.java.mode === "custom"
            ? instance.java.path
            : primaryJava.value?.home ?? javaSummary.value,
      };
    }
    if (row.name === "内存分配" && instance) {
      return {
        ...row,
        value: `最小 ${instance.memory.minimumMb / 1024} GB · 最大 ${instance.memory.maximumMb / 1024} GB`,
      };
    }
    if (row.name === "内存" && instance) {
      return { ...row, value: `${instance.memory.maximumMb / 1024} GB` };
    }
    if (row.name === "实例目录" && instance) {
      return {
        ...row,
        value: `instances\\${instance.id}\\${instance.gameDirectory}`,
      };
    }
    if (row.name === "启动模式" && instance) {
      const labels = {
        windowed: "窗口模式",
        maximized: "最大化",
        fullscreen: "全屏",
      };
      return { ...row, value: labels[instance.display.mode] };
    }
    if (row.name === "窗口尺寸" && instance) {
      return {
        ...row,
        value: `${instance.display.width} × ${instance.display.height}`,
      };
    }
    if (row.name === "JVM 参数" && instance) {
      return {
        ...row,
        value: instance.advanced.jvmArguments || "使用推荐参数",
      };
    }
    if (row.name === "游戏参数" && instance) {
      return {
        ...row,
        value: instance.advanced.gameArguments || "未设置",
      };
    }
    if (row.name === "调试模式" && instance) {
      return {
        ...row,
        value: instance.advanced.debugLogging ? "开启" : "关闭",
      };
    }
    if (row.name === "文件状态") {
      return {
        ...row,
        value: instance?.installation.state === "ready" ? "完整 · 已通过校验" : "安装不完整",
        action: "查看",
      };
    }
    if (row.name === "文件占用") {
      return { ...row, value: "尚未计算", action: "打开目录" };
    }
    return row;
  }),
);

function formatArchitecture(architecture?: string) {
  if (!architecture) return "";

  const normalized = architecture.toLowerCase();
  if (normalized === "amd64" || normalized === "x86_64") return "x64";
  if (normalized === "aarch64" || normalized === "arm64") return "ARM64";
  if (normalized === "x86" || normalized === "i386") return "x86";
  return architecture;
}

function setPage(page: Page) {
  currentPage.value = page;
  if (page === "downloads") void loadVersionCatalog(false);
  if (page === "logs") void loadLogs();
  if (page === "settings") void loadStorage();
  if (page === "help") {
    void loadDiagnostics();
    void loadStorage();
  }
}

function selectInstance(instance: LauncherInstance) {
  selectedInstanceId.value = instance.id;
  launcherSettings.value.selectedInstanceId = instance.id;
  currentPage.value = "instances";
  void invoke("select_instance", { instanceId: instance.id }).catch(() => {
    settingsBackendAvailable.value = false;
  });
}

function beginAddInstance() {
  setPage("downloads");
  downloadSection.value = "versions";
  selectedVersion.value = null;
  versionSearch.value = "";
}

function openInstallPanel(version: MinecraftVersion) {
  selectedVersion.value = version;
  installError.value = "";
  installProgress.value = null;
  installState.value = "idle";
  installCancelledByUser.value = false;
  installPanelOpen.value = true;
}

function openInstanceEditor() {
  if (!selectedInstance.value) return;
  instanceSaveError.value = "";
  instanceSaveState.value = "idle";
  instanceEditorOpen.value = true;
}

function closeInstanceEditor() {
  if (instanceSaveState.value === "saving") return;
  instanceEditorOpen.value = false;
  instanceSaveError.value = "";
}

async function saveInstance(instance: LauncherInstance) {
  instanceSaveState.value = "saving";
  instanceSaveError.value = "";
  try {
    const updated = await invoke<LauncherInstance>("update_instance", { instance });
    instances.value = instances.value
      .map((candidate) => (candidate.id === updated.id ? updated : candidate))
      .sort((left, right) => left.name.localeCompare(right.name, "zh-CN"));
    instanceEditorOpen.value = false;
  } catch (error) {
    instanceSaveError.value =
      typeof error === "string" ? error : "保存实例设置失败";
  } finally {
    instanceSaveState.value = "idle";
  }
}

function handleSettingAction() {
  if (currentTab.value === "files") {
    void openDirectory("instance");
    return;
  }
  openInstanceEditor();
}

function activeContextSection() {
  if (currentPage.value === "downloads") return downloadSection.value;
  if (currentPage.value === "settings") return settingsSection.value;
  if (currentPage.value === "logs") return logSection.value;
  if (currentPage.value === "help") return helpSection.value;
  return "";
}

function selectContextSection(section: string) {
  if (currentPage.value === "downloads") {
    downloadSection.value = section as DownloadSection;
  } else if (currentPage.value === "settings") {
    settingsSection.value = section as SettingsSection;
    settingsSaveError.value = "";
  } else if (currentPage.value === "logs") {
    logSection.value = section as LogSection;
  } else if (currentPage.value === "help") {
    helpSection.value = section as HelpSection;
  }
}

async function updateLauncherSettings(settings: LauncherSettings) {
  settingsSaveState.value = "saving";
  settingsSaveError.value = "";
  try {
    const updated = await invoke<LauncherSettings>("update_settings", { settings });
    launcherSettings.value = updated;
    theme.value = updated.theme;
    selectedInstanceId.value = updated.selectedInstanceId;
  } catch (error) {
    settingsSaveError.value =
      typeof error === "string" ? error : "保存全局设置失败";
  } finally {
    settingsSaveState.value = "idle";
  }
}

async function updateDownloadSettings(settings: DownloadSettings) {
  downloadSaveState.value = "saving";
  downloadSaveError.value = "";
  try {
    downloadSettings.value = await invoke<DownloadSettings>(
      "update_download_settings",
      { settings },
    );
  } catch (error) {
    downloadSaveError.value =
      typeof error === "string" ? error : "保存下载策略失败";
  } finally {
    downloadSaveState.value = "idle";
  }
}

async function loadLogs() {
  logsState.value = "loading";
  try {
    logs.value = await invoke<LogFile[]>("list_logs");
    logsState.value = "ready";
  } catch {
    logsState.value = "unavailable";
  }
}

async function openLog(log: LogFile) {
  logPanelOpen.value = true;
  logReadState.value = "loading";
  logReadError.value = "";
  activeLog.value = null;
  try {
    activeLog.value = await invoke<LogContent>("read_log", { name: log.name });
  } catch (error) {
    logReadError.value = typeof error === "string" ? error : "读取日志失败";
  } finally {
    logReadState.value = "idle";
  }
}

function formatFileSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatTransferRate(bytesPerSecond: number) {
  return `${formatFileSize(bytesPerSecond)}/s`;
}

function formatTimestamp(epochMs: number) {
  if (!epochMs) return "时间未知";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(epochMs));
}

function versionInstallCount(versionId: string) {
  return instances.value.filter(
    (instance) =>
      instance.gameVersion === versionId &&
      instance.installation.state === "ready",
  ).length;
}

function closeLogPanel() {
  logPanelOpen.value = false;
  activeLog.value = null;
  logReadError.value = "";
}

async function deleteActiveLog() {
  if (!activeLog.value || !window.confirm(`删除日志“${activeLog.value.name}”？`)) return;
  try {
    await invoke("delete_log", { name: activeLog.value.name });
    closeLogPanel();
    await loadLogs();
  } catch (error) {
    logReadError.value = typeof error === "string" ? error : "删除日志失败";
  }
}

async function clearOldLogs() {
  maintenanceState.value = "working";
  try {
    await invoke("clear_old_logs", {
      retentionDays: launcherSettings.value.logRetentionDays,
    });
    await loadLogs();
    maintenanceState.value = "done";
  } catch {
    maintenanceState.value = "error";
  }
}

async function loadDiagnostics() {
  diagnosticState.value = "loading";
  try {
    diagnostic.value = await invoke<DiagnosticReport>("diagnostics");
    diagnosticState.value = "ready";
  } catch {
    diagnosticState.value = "unavailable";
  }
}

async function loadStorage() {
  try {
    storage.value = await invoke<StorageReport>("storage_report");
  } catch {
    storage.value = null;
  }
}

async function cleanTemporaryDownloads() {
  maintenanceState.value = "working";
  try {
    await invoke<number>("clean_temporary_downloads");
    await loadStorage();
    maintenanceState.value = "done";
  } catch {
    maintenanceState.value = "error";
  }
}

async function openDirectory(
  target: "data" | "logs" | "cache" | "downloads" | "instance",
) {
  try {
    await invoke("open_directory", {
      target,
      instanceId: target === "instance" ? selectedInstance.value?.id ?? null : null,
    });
  } catch {
    // The visible page already exposes the path for manual access.
  }
}

async function copyDiagnostics() {
  if (!diagnostic.value) return;
  const report = [
    `NaCL ${diagnostic.value.appVersion}`,
    `${diagnostic.value.operatingSystem} ${diagnostic.value.architecture}`,
    `Java: ${diagnostic.value.javaRuntimeCount}`,
    `实例: ${diagnostic.value.instanceCount}`,
    `配置: ${diagnostic.value.roamingDataPath}`,
    `缓存: ${diagnostic.value.localDataPath}`,
  ].join("\n");
  try {
    await navigator.clipboard.writeText(report);
    copyState.value = "copied";
  } catch {
    copyState.value = "error";
  }
}

function closeInstallPanel() {
  if (installState.value !== "idle") return;
  installPanelOpen.value = false;
  installError.value = "";
}

async function loadVersionCatalog(forceRefresh: boolean) {
  if (versionState.value === "loading") return;
  if (!forceRefresh && versionCatalog.value) return;
  versionState.value = "loading";
  versionError.value = "";
  try {
    versionCatalog.value = await invoke<VersionCatalog>("load_version_catalog", {
      forceRefresh,
    });
    versionState.value = "ready";
  } catch (error) {
    versionState.value = "unavailable";
    versionError.value =
      typeof error === "string" ? error : "无法读取 Mojang 官方版本清单";
  }
}

async function installSelectedVersion(request: { name: string; versionId: string }) {
  installState.value = "installing";
  installCancelledByUser.value = false;
  installError.value = "";
  installProgress.value = null;
  try {
    const created = await invoke<LauncherInstance>("install_instance", {
      request,
    });
    instances.value = [...instances.value, created].sort((left, right) =>
      left.name.localeCompare(right.name, "zh-CN"),
    );
    selectedInstanceId.value = created.id;
    launcherSettings.value.selectedInstanceId = created.id;
    currentPage.value = "instances";
    installPanelOpen.value = false;
  } catch (error) {
    installError.value = installCancelledByUser.value
      ? ""
      : typeof error === "string"
        ? error
        : "安装失败，请检查网络后重试";
  } finally {
    installState.value = "idle";
  }
}

async function pauseInstall() {
  try {
    await invoke("pause_install");
    installState.value = "paused";
  } catch (error) {
    installError.value = typeof error === "string" ? error : "暂停失败";
  }
}

async function resumeInstall() {
  try {
    await invoke("resume_install");
    installState.value = "installing";
  } catch (error) {
    installError.value = typeof error === "string" ? error : "继续安装失败";
  }
}

async function cancelInstall() {
  if (!window.confirm("取消当前安装？已完成并校验的共享文件会保留，之后可复用。")) return;
  try {
    installCancelledByUser.value = true;
    await invoke("cancel_install");
  } catch (error) {
    installCancelledByUser.value = false;
    installError.value = typeof error === "string" ? error : "取消失败";
  }
}

async function duplicateSelectedInstance() {
  if (!selectedInstance.value) return;
  instanceSaveState.value = "saving";
  try {
    const duplicate = await invoke<LauncherInstance>("duplicate_instance", {
      instanceId: selectedInstance.value.id,
    });
    instances.value = [...instances.value, duplicate].sort((left, right) =>
      left.name.localeCompare(right.name, "zh-CN"),
    );
    selectedInstanceId.value = duplicate.id;
  } catch (error) {
    instanceSaveError.value = typeof error === "string" ? error : "复制实例失败";
  } finally {
    instanceSaveState.value = "idle";
  }
}

async function deleteSelectedInstance() {
  const instance = selectedInstance.value;
  if (!instance || !window.confirm(`删除实例“${instance.name}”及其存档和配置？此操作无法撤销。`)) return;
  try {
    const settings = await invoke<LauncherSettings>("delete_instance", {
      instanceId: instance.id,
    });
    instances.value = instances.value.filter((candidate) => candidate.id !== instance.id);
    launcherSettings.value = settings;
    selectedInstanceId.value = settings.selectedInstanceId;
  } catch (error) {
    instanceSaveError.value = typeof error === "string" ? error : "删除实例失败";
  }
}

function handleGlobalKeydown(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  if (installPanelOpen.value) {
    closeInstallPanel();
  } else if (instanceEditorOpen.value) {
    closeInstanceEditor();
  } else if (logPanelOpen.value) {
    closeLogPanel();
  }
}

function toggleTheme() {
  const nextTheme = theme.value === "dark" ? "light" : "dark";
  theme.value = nextTheme;
  launcherSettings.value = {
    ...launcherSettings.value,
    theme: nextTheme,
  };

  if (settingsBackendAvailable.value) {
    void invoke<LauncherSettings>("set_theme", { theme: nextTheme })
      .then((settings) => {
        launcherSettings.value = settings;
      })
      .catch(() => {
        settingsBackendAvailable.value = false;
      });
  }
}

async function runWindowAction(action: "minimize" | "maximize" | "close") {
  try {
    const appWindow = getCurrentWindow();
    if (action === "minimize") await appWindow.minimize();
    if (action === "maximize") await appWindow.toggleMaximize();
    if (action === "close") {
      if (launcherSettings.value.closeBehavior === "minimize") {
        await appWindow.minimize();
      } else {
        await appWindow.close();
      }
    }
  } catch {
    // Browser preview has no native Tauri window.
  }
}

async function startWindowResize(
  direction: ResizeDirection,
  event: MouseEvent,
) {
  if (event.button !== 0) return;
  event.preventDefault();
  event.stopPropagation();

  try {
    await getCurrentWindow().startResizeDragging(direction);
  } catch {
    // Browser preview has no native Tauri window.
  }
}

async function loadJavaRuntimes() {
  javaDetectionState.value = "loading";
  javaError.value = "";
  try {
    javaRuntimes.value = await invoke<JavaRuntime[]>("detect_java_runtimes");
    javaDetectionState.value = "ready";
  } catch (error) {
    javaDetectionState.value = "unavailable";
    javaError.value =
      typeof error === "string" ? error : "Java 检测暂不可用";
  }
}

async function browseJavaRuntime() {
  javaError.value = "";
  try {
    const selected = await open({
      multiple: false,
      directory: false,
      title: "选择 java.exe",
      filters: [{ name: "Java 运行程序", extensions: ["exe"] }],
    });
    if (!selected) return;
    const runtime = await invoke<JavaRuntime>("inspect_java_runtime", {
      path: selected,
    });
    javaRuntimes.value = [
      runtime,
      ...javaRuntimes.value.filter(
        (candidate) => candidate.path !== runtime.path,
      ),
    ];
    await updateLauncherSettings({
      ...launcherSettings.value,
      preferredJavaPath: runtime.path,
    });
  } catch (error) {
    javaError.value =
      typeof error === "string" ? error : "无法使用所选 Java";
  }
}

async function installManagedJava(majorVersion: number) {
  javaDownloadState.value = "downloading";
  javaError.value = "";
  try {
    const runtime = await invoke<JavaRuntime>("install_managed_java", {
      majorVersion,
    });
    await loadJavaRuntimes();
    await updateLauncherSettings({
      ...launcherSettings.value,
      preferredJavaPath: runtime.path,
      manageRuntimes: true,
    });
  } catch (error) {
    javaError.value =
      typeof error === "string" ? error : `Java ${majorVersion} 下载失败`;
  } finally {
    javaDownloadState.value = "idle";
  }
}

async function loadMemoryReport() {
  memoryState.value = "loading";
  try {
    memoryReport.value = await invoke<MemoryReport>("memory_report");
    memoryState.value = "ready";
  } catch {
    memoryReport.value = null;
    memoryState.value = "unavailable";
  }
}

async function loadAppBootstrap() {
  try {
    const bootstrap = await invoke<AppBootstrap>("bootstrap_app");
    launcherSettings.value = bootstrap.settings;
    downloadSettings.value = bootstrap.downloadSettings;
    appPaths.value = bootstrap.paths;
    theme.value = bootstrap.settings.theme;
    selectedInstanceId.value = bootstrap.settings.selectedInstanceId;
    currentPage.value = bootstrap.settings.defaultPage;
    settingsBackendAvailable.value = true;
  } catch {
    settingsBackendAvailable.value = false;
  }
}

async function loadInstances() {
  instancesState.value = "loading";
  try {
    instances.value = await invoke<LauncherInstance[]>("list_instances");
    if (
      !selectedInstanceId.value ||
      !instances.value.some((instance) => instance.id === selectedInstanceId.value)
    ) {
      selectedInstanceId.value = instances.value[0]?.id ?? null;
    }
    instancesState.value = "ready";
  } catch {
    instancesState.value = "unavailable";
  }
}

onMounted(async () => {
  document.documentElement.dataset.theme = theme.value;
  window.addEventListener("keydown", handleGlobalKeydown);
  unlistenInstallProgress = await listen<InstallProgress>(
    "install-progress",
    (event) => {
      installProgress.value = event.payload;
    },
  ).catch(() => null);
  await loadAppBootstrap();
  await loadInstances();
  void loadJavaRuntimes();
  void loadMemoryReport();
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", handleGlobalKeydown);
  unlistenInstallProgress?.();
});

watch(theme, (nextTheme) => {
  document.documentElement.dataset.theme = nextTheme;
});

watch(
  () => launcherSettings.value,
  (settings) => {
    document.documentElement.dataset.motion = settings.animationsEnabled
      ? settings.animationSpeed
      : "off";
    document.documentElement.dataset.density = settings.interfaceDensity;
    document.documentElement.dataset.smileyHeadings = settings.useSmileyHeadings
      ? "on"
      : "off";
  },
  { deep: true, immediate: true },
);
</script>

<template>
  <div class="app-shell">
    <div
      v-for="handle in resizeHandles"
      :key="handle.direction"
      class="resize-handle"
      :class="`resize-handle--${handle.position}`"
      aria-hidden="true"
      @mousedown="startWindowResize(handle.direction, $event)"
    ></div>

    <aside class="rail" aria-label="主导航">
      <button class="brand-button" aria-label="NaCL 首页" @click="setPage('home')">
        <img :src="brandIcon" alt="" />
      </button>

      <nav class="rail-nav">
        <button
          v-for="item in railItems"
          :key="item.label"
          class="icon-button"
          :class="{ active: item.page === currentPage }"
          :aria-label="item.label"
          :title="item.label"
          @click="setPage(item.page)"
        >
          <FlatIcon :name="item.icon" />
        </button>
      </nav>

      <div class="rail-bottom">
        <button
          class="icon-button"
          :class="{ active: currentPage === 'help' }"
          aria-label="帮助"
          title="帮助"
          @click="setPage('help')"
        >
          <FlatIcon name="help" />
        </button>
        <button
          class="icon-button"
          :class="{ active: currentPage === 'settings' }"
          aria-label="全局设置"
          title="全局设置"
          @click="setPage('settings')"
        >
          <FlatIcon name="settings" />
        </button>
      </div>
    </aside>

    <aside class="context-sidebar" aria-label="上下文导航">
      <div>
        <div class="context-title">{{ pageContext.title }}</div>
        <div class="context-label">{{ pageContext.label }}</div>
        <div v-if="currentPage === 'home' || currentPage === 'instances'" class="instance-list">
          <button
            v-for="instance in instances"
            :key="instance.id"
            class="instance-item"
            :class="{ active: instance.id === selectedInstance?.id }"
            @click="selectInstance(instance)"
          >
            <span class="instance-mark"><FlatIcon name="instances" /></span>
            <span class="instance-copy">
              <span class="instance-name">{{ instance.name }}</span>
              <span class="instance-version">Java Edition · {{ instance.gameVersion }}</span>
            </span>
          </button>
          <div v-if="instancesState === 'loading'" class="instance-empty">正在读取实例…</div>
          <div v-else-if="instancesState === 'unavailable'" class="instance-empty">桌面端读取不可用</div>
          <div v-else-if="instances.length === 0" class="instance-empty">还没有本地实例</div>
        </div>
        <button
          v-if="currentPage === 'home' || currentPage === 'instances'"
          class="new-instance"
          @click="beginAddInstance"
        >
          <FlatIcon name="plus" />
          <span>添加实例</span>
        </button>
        <nav v-else class="context-links">
          <button
            v-for="item in pageContext.items"
            :key="item.label"
            class="context-link"
            :class="{ active: activeContextSection() === item.section }"
            @click="selectContextSection(item.section)"
          >
            <span>{{ item.label }}</span>
          </button>
        </nav>
      </div>

      <div class="account-summary">
        <div class="avatar"><FlatIcon name="user" /></div>
        <div>
          <div class="account-name">本地档案</div>
          <div class="account-status">账号功能预留</div>
        </div>
      </div>
    </aside>

    <main class="main-panel">
      <header class="titlebar">
        <div
          class="titlebar-drag"
          data-tauri-drag-region
        >
          <span class="window-brand-short" data-tauri-drag-region>NaCL</span>
          <span class="window-brand-divider" data-tauri-drag-region></span>
          <span class="window-title" data-tauri-drag-region>Na Craft Launcher</span>
        </div>
        <div class="window-controls">
          <button
            class="theme-toggle"
            :aria-label="theme === 'dark' ? '切换到亮色主题' : '切换到暗色主题'"
            :title="theme === 'dark' ? '亮色主题' : '暗色主题'"
            @click="toggleTheme"
          >
            <FlatIcon :name="theme === 'dark' ? 'moon' : 'sun'" />
          </button>
          <button class="window-control" aria-label="最小化" @click="runWindowAction('minimize')">
            <FlatIcon name="minimize" />
          </button>
          <button class="window-control" aria-label="最大化" @click="runWindowAction('maximize')">
            <FlatIcon name="maximize" />
          </button>
          <button class="window-control close" aria-label="关闭" @click="runWindowAction('close')">
            <FlatIcon name="close" />
          </button>
        </div>
      </header>

      <Transition name="page" mode="out-in">
      <section v-if="currentPage === 'home'" key="home" class="content home-page">
        <div class="content-head">
          <div>
            <div class="eyebrow">快速启动</div>
            <h1 class="page-heading">{{ selectedInstance?.name ?? "创建第一个实例" }}</h1>
            <div class="home-subtitle">
              <template v-if="selectedInstance">
                Minecraft {{ selectedInstance.gameVersion }} · {{ javaSummary }} ·
                {{ selectedInstance.installation.state === "ready" ? "已安装" : "安装不完整" }}
              </template>
              <template v-else>选择版本并建立独立的游戏目录</template>
            </div>
          </div>
        </div>

        <div class="home-grid">
          <div class="quick-launch">
            <div class="quick-specs">
              <div class="quick-spec">
                <div class="spec-label">游戏版本</div>
                <div class="spec-value">
                  {{ selectedInstance ? `Minecraft ${selectedInstance.gameVersion}` : "未选择" }}
                </div>
              </div>
              <div class="quick-spec">
                <div class="spec-label">运行环境</div>
                <div class="spec-value">{{ javaSummary }}</div>
              </div>
              <div class="quick-spec">
                <div class="spec-label">内存分配</div>
                <div class="spec-value">
                  {{ selectedInstance ? `${selectedInstance.memory.maximumMb / 1024} GB` : "自动" }}
                </div>
              </div>
            </div>
            <div class="quick-actions">
              <button
                v-if="selectedInstance"
                class="launch-button"
                :disabled="selectedInstance.installation.state !== 'ready'"
                :title="selectedInstance.installation.state === 'ready' ? '账号接入后可启动' : '请从下载页重新安装'"
              >
                <FlatIcon name="download" />
                <span>{{ selectedInstance.installation.state === "ready" ? "等待账号接入" : "安装不完整" }}</span>
              </button>
              <button v-else class="launch-button" @click="beginAddInstance">
                <FlatIcon name="plus" />
                <span>添加实例</span>
              </button>
              <button
                v-if="selectedInstance"
                class="text-action"
                @click="setPage('instances')"
              >
                查看实例设置
              </button>
            </div>
          </div>

          <div class="profile-preview">
            <div class="profile-label">当前账号</div>
            <div class="profile-main">
              <div class="skin-avatar" aria-label="玩家头像预览">
                <svg viewBox="0 0 16 16" shape-rendering="crispEdges" role="img" aria-label="占位玩家头像">
                  <rect width="16" height="16" fill="#1b777b" />
                  <rect x="2" y="2" width="12" height="12" fill="#d8a980" />
                  <rect x="2" y="2" width="12" height="3" fill="#20292b" />
                  <rect x="3" y="5" width="3" height="2" fill="#20292b" />
                  <rect x="10" y="5" width="3" height="2" fill="#20292b" />
                  <rect x="4" y="7" width="2" height="2" fill="#f2f6f6" />
                  <rect x="10" y="7" width="2" height="2" fill="#f2f6f6" />
                  <rect x="5" y="8" width="1" height="1" fill="#17585c" />
                  <rect x="10" y="8" width="1" height="1" fill="#17585c" />
                  <rect x="6" y="11" width="4" height="1" fill="#8f5f50" />
                  <rect y="13" width="16" height="3" fill="#17585c" />
                </svg>
              </div>
              <div>
                <div class="profile-name">本地档案</div>
                <div class="provider"><span class="provider-dot"></span>账号功能预留</div>
                <div class="account-status">尚未连接</div>
              </div>
            </div>
            <div class="profile-actions">
              <button class="profile-action" disabled>连接 Microsoft</button>
              <button class="profile-action" disabled>离线登录</button>
            </div>
          </div>
        </div>

        <div class="section">
          <div class="section-header">
            <div class="section-title">最近活动</div>
            <div class="section-note">本地实例</div>
          </div>
          <div v-if="selectedInstance" class="activity-row">
            <span class="activity-dot activity-dot--idle"></span>
            <span class="activity-title">上次启动</span>
            <span>{{ selectedInstance.name }} · 尚未启动</span>
            <span>—</span>
          </div>
          <div v-else class="activity-empty">
            创建实例后，启动记录会显示在这里。
          </div>
        </div>
      </section>

      <section
        v-else-if="currentPage === 'instances' && selectedInstance"
        :key="`instances-${selectedInstance.id}-${currentTab}`"
        class="content instance-page"
      >
        <div class="content-head">
          <div>
            <div class="eyebrow">Java Edition · {{ selectedInstance.gameVersion }}</div>
            <h1 class="page-heading">{{ selectedInstance.name }}</h1>
          </div>
          <div class="head-actions">
            <button class="setting-action" :disabled="maintenanceState === 'working'" @click="clearOldLogs">
              清理旧日志
            </button>
            <button class="setting-action" :disabled="instanceSaveState === 'saving'" @click="duplicateSelectedInstance">
              复制
            </button>
            <button class="setting-action danger-action" @click="deleteSelectedInstance">删除</button>
            <button
              class="launch-button"
              :disabled="selectedInstance.installation.state !== 'ready'"
              title="Microsoft 账号接入后开放启动"
            >
              <FlatIcon name="download" />
              <span>{{ selectedInstance.installation.state === "ready" ? "等待账号" : "安装不完整" }}</span>
            </button>
          </div>
        </div>

        <nav class="instance-tabs" aria-label="实例设置分类">
          <button
            v-for="tab in tabs"
            :key="tab.id"
            class="instance-tab"
            :class="{ active: tab.id === currentTab }"
            @click="currentTab = tab.id"
          >
            {{ tab.label }}
          </button>
        </nav>

        <div class="settings-list">
          <div v-for="row in visibleSettings" :key="row.name" class="setting-row">
            <div>
              <div class="setting-name">{{ row.name }}</div>
              <div class="setting-description">{{ row.description }}</div>
            </div>
            <div class="setting-value">{{ row.value }}</div>
            <button class="setting-action" @click="handleSettingAction">{{ row.action }}</button>
          </div>
        </div>
      </section>

      <section
        v-else-if="currentPage === 'instances'"
        key="instances-empty"
        class="content instance-empty-page"
      >
        <div class="empty-mark"><FlatIcon name="instances" /></div>
        <div class="eyebrow">Java Edition</div>
        <h1 class="empty-heading">还没有实例</h1>
        <p>创建一个原版实例后，可在这里管理版本、Java、内存与游戏目录。</p>
        <button class="launch-button" @click="beginAddInstance">
          <FlatIcon name="plus" />
          <span>添加实例</span>
        </button>
      </section>

      <section
        v-else-if="currentPage === 'downloads'"
        :key="`downloads-${downloadSection}`"
        class="content feature-page"
      >
        <div class="content-head">
          <div>
            <div class="eyebrow">Minecraft Java Edition</div>
            <h1 class="page-heading">
              {{ downloadSection === "versions" ? "版本浏览" : downloadSection === "queue" ? "安装队列" : "下载策略" }}
            </h1>
            <div class="home-subtitle">
              {{
                downloadSection === "versions"
                  ? "选择 Mojang 官方版本并创建独立实例"
                  : downloadSection === "queue"
                    ? "查看进行中的任务与最近完成的安装"
                    : "只影响 Minecraft 文件下载，不属于全局设置"
              }}
            </div>
          </div>
          <div v-if="downloadSection === 'versions'" class="head-actions">
            <button class="icon-button" aria-label="刷新版本清单" :disabled="versionState === 'loading'" @click="loadVersionCatalog(true)">
              <FlatIcon name="refresh" />
            </button>
          </div>
          <button
            v-else-if="downloadSection === 'queue'"
            class="setting-action"
            @click="openDirectory('downloads')"
          >
            打开下载目录
          </button>
        </div>

        <article v-if="downloadSection === 'versions'" class="feature-card version-browser">
            <div class="feature-card-head">
              <div>
                <div class="feature-kicker">版本浏览</div>
                <h2>Mojang 官方版本</h2>
              </div>
              <div class="version-list-options">
                <span v-if="versionCatalog" class="catalog-source">
                  {{ versionCatalog.source === "network" ? "在线清单" : "本地缓存" }}
                </span>
                <label class="compact-switch">
                  <input
                    type="checkbox"
                    :checked="downloadSettings.showSnapshots"
                    :disabled="downloadSaveState === 'saving'"
                    @change="updateDownloadSettings({ ...downloadSettings, showSnapshots: !downloadSettings.showSnapshots })"
                  />
                  <span>显示快照版本</span>
                </label>
              </div>
            </div>
            <input v-model="versionSearch" class="search-input" placeholder="搜索版本，例如 1.21" />
            <div v-if="versionState === 'loading'" class="data-empty">正在读取官方版本清单…</div>
            <div v-else-if="versionState === 'unavailable'" class="data-empty">
              {{ versionError || "版本服务暂不可用" }}
            </div>
            <div v-else class="version-list">
              <div class="version-list-head" aria-hidden="true">
                <span>版本</span>
                <span>发布频道</span>
                <span>发布时间</span>
                <span>本地状态</span>
                <span></span>
              </div>
              <button
                v-for="version in visibleVersions"
                :key="version.id"
                class="version-row"
                @click="openInstallPanel(version)"
              >
                <strong>{{ version.id }}</strong>
                <span class="version-type">{{ version.type === "release" ? "正式版" : "快照版" }}</span>
                <time :datetime="version.releaseTime">{{ formatTimestamp(Date.parse(version.releaseTime)) }}</time>
                <span class="version-local-state">
                  {{ versionInstallCount(version.id) ? `已安装 ${versionInstallCount(version.id)} 个实例` : "未安装" }}
                </span>
                <span class="version-action">{{ versionInstallCount(version.id) ? "再次安装" : "安装" }}</span>
              </button>
              <div v-if="visibleVersions.length === 0 && versionState === 'ready'" class="data-empty">没有匹配的版本</div>
            </div>
        </article>

        <div v-else-if="downloadSection === 'queue'" class="queue-page">
          <article v-if="installState !== 'idle'" class="queue-task">
            <div class="queue-task-main">
              <span class="activity-dot" :class="{ 'activity-dot--idle': installState === 'paused' }"></span>
              <div>
                <div class="feature-kicker">{{ installState === "paused" ? "已暂停" : "正在安装" }}</div>
                <h2>Minecraft {{ selectedVersion?.id }}</h2>
                <p>{{ installProgress?.currentFile || "正在连接 Mojang 服务…" }}</p>
              </div>
            </div>
            <div class="queue-progress">
              <div class="install-progress-head">
                <span>{{ installProgress?.completedFiles ?? 0 }} / {{ installProgress?.totalFiles ?? 0 }} 个文件</span>
                <strong>{{ installProgressPercent }}%</strong>
              </div>
              <div class="progress-track"><span :style="{ width: `${installProgressPercent}%` }"></span></div>
              <span>
                {{ formatFileSize(installProgress?.downloadedBytes ?? 0) }} 已下载 ·
                {{ installState === "paused" ? "已暂停" : formatTransferRate(installProgress?.downloadSpeedBytesPerSecond ?? 0) }}
              </span>
            </div>
            <div class="queue-actions">
              <button v-if="installState === 'installing'" class="setting-action" @click="pauseInstall">暂停</button>
              <button v-else class="setting-action" @click="resumeInstall">继续</button>
              <button class="setting-action danger-action" @click="cancelInstall">取消</button>
            </div>
          </article>
          <div v-else class="content-empty-block">
            <FlatIcon name="download" />
            <h2>没有进行中的安装</h2>
            <p>从“版本浏览”选择版本后，任务进度会显示在这里。</p>
            <button class="setting-action" @click="downloadSection = 'versions'">浏览版本</button>
          </div>

          <div class="section queue-history">
            <div class="section-header">
              <div class="section-title">已完成安装</div>
              <div class="section-note">{{ installedInstances.length }} 个实例</div>
            </div>
            <div v-if="installedInstances.length === 0" class="activity-empty">还没有完成的安装记录。</div>
            <template v-else>
              <button
                v-for="instance in installedInstances"
                :key="instance.id"
                class="activity-row queue-history-row"
                @click="selectInstance(instance)"
              >
                <span class="activity-dot"></span>
                <span class="activity-title">{{ instance.name }}</span>
                <span>Minecraft {{ instance.gameVersion }}</span>
                <span>{{ formatTimestamp(instance.installation.installedEpochMs ?? 0) }}</span>
              </button>
            </template>
          </div>
        </div>

        <DownloadStrategyPanel
          v-else
          :settings="downloadSettings"
          :saving="downloadSaveState === 'saving'"
          :error="downloadSaveError"
          @update="updateDownloadSettings"
        />
      </section>

      <section
        v-else-if="currentPage === 'logs'"
        :key="`logs-${logSection}`"
        class="content feature-page"
      >
        <div class="content-head">
          <div>
            <div class="eyebrow">本地记录</div>
            <h1 class="page-heading">{{ logSection === "launcher" ? "启动器日志" : "游戏日志" }}</h1>
            <div class="home-subtitle">
              {{ logSection === "launcher" ? "查看 NaCL 运行和安装过程产生的记录" : "按实例查看 Minecraft 进程输出" }}
            </div>
          </div>
          <div v-if="logSection === 'launcher'" class="head-actions">
            <button class="icon-button" aria-label="刷新日志" @click="loadLogs">
              <FlatIcon name="refresh" />
            </button>
            <button class="setting-action" @click="openDirectory('logs')">打开目录</button>
          </div>
        </div>

        <template v-if="logSection === 'launcher'">
          <input v-model="logSearch" class="search-input log-search" placeholder="搜索日志文件名" />
          <div class="data-list">
          <div v-if="logsState === 'loading'" class="data-empty">正在读取日志…</div>
          <div v-else-if="logsState === 'unavailable'" class="data-empty">桌面端日志读取不可用</div>
          <div v-else-if="visibleLogs.length === 0" class="data-empty">
            暂无日志。启动器和游戏功能开始运行后，记录会显示在这里。
          </div>
          <template v-else>
            <button
              v-for="log in visibleLogs"
              :key="log.name"
              class="data-row"
              @click="openLog(log)"
            >
              <span class="data-row-icon"><FlatIcon name="file" /></span>
              <span>
                <strong>{{ log.name }}</strong>
                <small>{{ formatFileSize(log.sizeBytes) }}</small>
              </span>
              <span class="data-row-meta">{{ formatTimestamp(log.modifiedEpochMs) }}</span>
            </button>
          </template>
          </div>
        </template>
        <div v-else class="content-empty-block">
          <FlatIcon name="file" />
          <h2>还没有游戏日志</h2>
          <p>完成账号与游戏启动功能后，这里会按实例显示 Minecraft 输出。</p>
        </div>
      </section>

      <section
        v-else-if="currentPage === 'settings'"
        :key="`settings-${settingsSection}`"
        class="content feature-page"
      >
        <div class="content-head">
          <div>
            <div class="eyebrow">NaCL</div>
            <h1 class="page-heading">{{ settingsSectionTitle }}</h1>
            <div class="home-subtitle">
              {{
                settingsSection === "storage"
                  ? "查看本地目录并清理临时文件"
                  : settingsSection === "defaults"
                    ? "只影响之后安装的新实例"
                    : "全局设置会自动保存"
              }}
            </div>
          </div>
        </div>

        <div v-if="settingsSection === 'storage'" class="storage-summary">
          <div><span>配置</span><strong>{{ formatFileSize(storage?.configurationBytes ?? 0) }}</strong></div>
          <div><span>实例</span><strong>{{ formatFileSize(storage?.instanceBytes ?? 0) }}</strong></div>
          <div><span>共享缓存</span><strong>{{ formatFileSize(storage?.cacheBytes ?? 0) }}</strong></div>
          <div><span>运行环境</span><strong>{{ formatFileSize(storage?.runtimeBytes ?? 0) }}</strong></div>
        </div>
        <SettingsSectionPanel
          :settings="launcherSettings"
          :section="settingsSection"
          :saving="settingsSaveState === 'saving' || maintenanceState === 'working'"
          :error="settingsSaveError"
          :java-runtimes="javaRuntimes"
          :java-detection-state="javaDetectionState"
          :java-download-state="javaDownloadState"
          :java-error="javaError"
          :memory-report="memoryReport"
          :memory-state="memoryState"
          @update="updateLauncherSettings"
          @open-directory="openDirectory"
          @clean-downloads="cleanTemporaryDownloads"
          @rescan-java="loadJavaRuntimes"
          @browse-java="browseJavaRuntime"
          @install-java="installManagedJava"
        />
      </section>

      <section v-else :key="`help-${helpSection}`" class="content feature-page">
        <div class="content-head">
          <div>
            <div class="eyebrow">支持</div>
            <h1 class="page-heading">
              {{ helpSection === "diagnostics" ? "环境诊断" : helpSection === "directories" ? "数据目录" : "关于 NaCL" }}
            </h1>
            <div class="home-subtitle">
              {{
                helpSection === "diagnostics"
                  ? "检查系统、Java 与本地实例概况"
                  : helpSection === "directories"
                    ? "查看 NaCL 的配置、缓存和日志位置"
                    : "Na Craft Launcher 项目信息"
              }}
            </div>
          </div>
          <button
            v-if="helpSection === 'diagnostics'"
            class="setting-action"
            :disabled="!diagnostic"
            @click="copyDiagnostics"
          >
            {{ copyState === "copied" ? "已复制" : "复制诊断信息" }}
          </button>
        </div>

        <template v-if="helpSection === 'diagnostics'">
          <div v-if="diagnosticState === 'loading'" class="data-empty">正在生成诊断信息…</div>
          <div v-else-if="diagnosticState === 'unavailable'" class="data-empty">
            桌面端诊断不可用
          </div>
          <div v-else-if="diagnostic" class="diagnostic-grid">
          <article class="feature-card">
            <div class="feature-kicker">应用</div>
            <h2>NaCL {{ diagnostic.appVersion }}</h2>
            <p>{{ diagnostic.operatingSystem }} · {{ diagnostic.architecture }}</p>
          </article>
          <article class="feature-card">
            <div class="feature-kicker">运行环境</div>
            <div class="metric-value">{{ diagnostic.javaRuntimeCount }}</div>
            <div class="metric-label">个 Java 运行环境</div>
          </article>
          <article class="feature-card">
            <div class="feature-kicker">本地实例</div>
            <div class="metric-value">{{ diagnostic.instanceCount }}</div>
            <div class="metric-label">个有效实例</div>
          </article>
          <article class="feature-card feature-card--wide path-card">
            <div class="feature-kicker">用户数据</div>
            <code>{{ diagnostic.roamingDataPath }}</code>
            <div class="card-actions">
              <button class="text-action" @click="openDirectory('data')">打开配置目录</button>
              <button class="text-action" @click="openDirectory('cache')">打开缓存目录</button>
              <button class="text-action" @click="openDirectory('logs')">打开日志目录</button>
            </div>
          </article>
          </div>
        </template>
        <div v-else-if="helpSection === 'directories'" class="directory-list">
          <div class="setting-control">
            <span><strong>用户数据</strong><small>{{ diagnostic?.roamingDataPath ?? appPaths?.roamingRoot ?? "桌面端读取" }}</small></span>
            <button class="setting-action" @click="openDirectory('data')">打开</button>
          </div>
          <div class="setting-control">
            <span><strong>共享缓存</strong><small>{{ diagnostic?.localDataPath ?? appPaths?.localRoot ?? "桌面端读取" }}</small></span>
            <button class="setting-action" @click="openDirectory('cache')">打开</button>
          </div>
          <div class="setting-control">
            <span><strong>日志</strong><small>{{ appPaths?.logsDir ?? "桌面端读取" }}</small></span>
            <button class="setting-action" @click="openDirectory('logs')">打开</button>
          </div>
          <div class="storage-summary">
            <div><span>实例</span><strong>{{ formatFileSize(storage?.instanceBytes ?? 0) }}</strong></div>
            <div><span>缓存</span><strong>{{ formatFileSize(storage?.cacheBytes ?? 0) }}</strong></div>
            <div><span>日志</span><strong>{{ formatFileSize(storage?.logBytes ?? 0) }}</strong></div>
          </div>
        </div>
        <div v-else class="about-panel">
          <img :src="brandIcon" alt="" />
          <div>
            <div class="eyebrow">NaCL</div>
            <h2>Na Craft Launcher</h2>
            <p>面向 Windows 的极简 Minecraft Java Edition 启动器。</p>
            <p>当前版本 {{ diagnostic?.appVersion ?? "0.1.0" }} · 原版 Vanilla</p>
          </div>
        </div>
      </section>
      </Transition>

      <div v-if="currentPage === 'instances' && selectedInstance" class="resource-status">
        <FlatIcon name="file" />
        <span>实例配置</span>
        <strong>{{ selectedInstance.installation.state === "ready" ? "文件完整" : "安装不完整" }}</strong>
      </div>
    </main>

    <InstallInstanceDrawer
      :open="installPanelOpen"
      :version="selectedVersion"
      :installing="installState === 'installing'"
      :paused="installState === 'paused'"
      :progress="installProgress"
      :error="installError"
      :default-memory-mb="launcherSettings.defaultMemoryMb"
      @close="closeInstallPanel"
      @install="installSelectedVersion"
      @pause="pauseInstall"
      @resume="resumeInstall"
      @cancel="cancelInstall"
    />
    <InstanceSettingsDrawer
      :open="instanceEditorOpen"
      :instance="selectedInstance"
      :tab="currentTab"
      :saving="instanceSaveState === 'saving'"
      :error="instanceSaveError"
      @close="closeInstanceEditor"
      @save="saveInstance"
      @open-directory="openDirectory('instance')"
      @browse-versions="instanceEditorOpen = false; beginAddInstance()"
    />
    <LogDetailDrawer
      :open="logPanelOpen"
      :log="activeLog"
      :loading="logReadState === 'loading'"
      :error="logReadError"
      @close="closeLogPanel"
      @delete="deleteActiveLog"
    />
  </div>
</template>
