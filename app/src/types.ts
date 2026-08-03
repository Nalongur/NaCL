export type Page =
  | "home"
  | "instances"
  | "downloads"
  | "logs"
  | "settings"
  | "help";

export type Theme = "dark" | "light";
export type DefaultPage = "home" | "instances" | "downloads";
export type SettingsSection =
  | "general"
  | "appearance"
  | "java"
  | "defaults"
  | "storage"
  | "logs";
export type InstanceTab =
  | "overview"
  | "version"
  | "runtime"
  | "display"
  | "files"
  | "advanced";

export interface LauncherSettings {
  schemaVersion: number;
  theme: Theme;
  selectedInstanceId: string | null;
  defaultPage: DefaultPage;
  rememberLastInstance: boolean;
  closeBehavior: "exit" | "minimize";
  checkUpdates: boolean;
  notifications: boolean;
  animationsEnabled: boolean;
  animationSpeed: "fast" | "normal" | "relaxed";
  interfaceDensity: "compact" | "comfortable";
  useSmileyHeadings: boolean;
  autoDetectJava: boolean;
  preferredJavaPath: string | null;
  manageRuntimes: boolean;
  compatibilityWarnings: boolean;
  defaultMemoryMb: number;
  defaultWindowWidth: number;
  defaultWindowHeight: number;
  logLevel: "error" | "warn" | "info" | "debug";
  logRetentionDays: number;
  autoDiagnostics: boolean;
}

export interface DownloadSettings {
  schemaVersion: number;
  concurrentDownloads: number;
  connectionsPerDownload: number;
  segmentedDownloadThresholdMib: number;
  retryCount: number;
  connectionTimeoutSeconds: number;
  speedLimitKibPerSecond: number;
  showSnapshots: boolean;
  verifyAfterDownload: boolean;
}

export interface LauncherInstance {
  schemaVersion: number;
  id: string;
  name: string;
  gameVersion: string;
  loader: "vanilla";
  gameDirectory: string;
  java: { mode: "auto" } | { mode: "custom"; path: string };
  memory: {
    minimumMb: number;
    maximumMb: number;
  };
  display: {
    mode: "windowed" | "maximized" | "fullscreen";
    width: number;
    height: number;
  };
  advanced: {
    jvmArguments: string;
    gameArguments: string;
    debugLogging: boolean;
  };
  installation: {
    state: "incomplete" | "ready";
    installedEpochMs: number | null;
  };
}

export interface LogFile {
  name: string;
  sizeBytes: number;
  modifiedEpochMs: number;
}

export interface LogContent {
  name: string;
  content: string;
  truncated: boolean;
}

export interface DiagnosticReport {
  appVersion: string;
  operatingSystem: string;
  architecture: string;
  javaRuntimeCount: number;
  instanceCount: number;
  roamingDataPath: string;
  localDataPath: string;
}

export interface JavaRuntime {
  path: string;
  home: string;
  version?: string;
  majorVersion?: number;
  architecture?: string;
  vendor?: string;
  source: string;
  managed: boolean;
}

export interface MemoryReport {
  totalBytes: number;
  availableBytes: number;
  recommendedMb: number;
  maximumAssignableMb: number;
}

export interface StorageReport {
  configurationBytes: number;
  instanceBytes: number;
  cacheBytes: number;
  runtimeBytes: number;
  logBytes: number;
}

export interface StoragePathSettings {
  schemaVersion: number;
  instancesDirectory: string | null;
  cacheDirectory: string | null;
}

export interface AppPaths {
  roamingRoot: string;
  configDir: string;
  settingsFile: string;
  downloadSettingsFile: string;
  storagePathsFile: string;
  instancesDir: string;
  localRoot: string;
  cacheDir: string;
  manifestsDir: string;
  versionsDir: string;
  assetsDir: string;
  librariesDir: string;
  downloadsDir: string;
  runtimesDir: string;
  logsDir: string;
}

export interface StoragePathConfiguration {
  paths: AppPaths;
  storagePaths: StoragePathSettings;
}

export interface MinecraftVersion {
  id: string;
  type: "release" | "snapshot" | "old_beta" | "old_alpha";
  url: string;
  releaseTime: string;
  sha1: string;
  complianceLevel: number;
}

export interface VersionCatalog {
  latestRelease: string;
  latestSnapshot: string;
  versions: MinecraftVersion[];
  source: "network" | "cache";
  refreshedEpochMs: number;
}

export interface InstallProgress {
  stage: "metadata" | "libraries" | "assets" | "finalizing" | "complete";
  completedFiles: number;
  totalFiles: number;
  currentFile: string;
  downloadedBytes: number;
  downloadSpeedBytesPerSecond: number;
  downloadEngine: "cache" | "segmented" | "streaming";
  activeConnections: number;
}

export interface OfflineProfile {
  schemaVersion: number;
  username: string;
  uuid: string;
}

export interface MinecraftSkin {
  id: string;
  state: string;
  url: string;
  variant: string;
}

export interface MinecraftCape {
  id: string;
  state: string;
  url: string;
  alias: string;
}

export interface MicrosoftAccount {
  schemaVersion: number;
  minecraftId: string;
  minecraftName: string;
  skins: MinecraftSkin[];
  capes: MinecraftCape[];
  signedInEpochMs: number;
}

export interface LaunchResult {
  instanceId: string;
  processId: number;
  startedEpochMs: number;
  logFile: string;
}

export interface GameExited {
  instanceId: string;
  exitCode: number | null;
}
