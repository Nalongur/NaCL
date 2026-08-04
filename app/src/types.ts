export type Page =
  | "home"
  | "instances"
  | "downloads"
  | "logs"
  | "settings"
  | "help";

export type Theme = "dark" | "light";
export type LoaderKind = "vanilla" | "fabric" | "quilt" | "forge" | "neoforge";
export type ContentKind = "mod" | "resourcepack" | "shader" | "datapack" | "modpack";
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
  | "content"
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
  loader: {
    kind: LoaderKind;
    version?: string;
    profileId?: string;
  };
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

export interface LoaderVersion {
  version: string;
  stable: boolean;
  recommended: boolean;
}

export interface LoaderCatalog {
  gameVersion: string;
  loader: LoaderKind;
  versions: LoaderVersion[];
}

export interface ContentProject {
  projectId: string;
  slug: string;
  title: string;
  description: string;
  author: string;
  iconUrl: string | null;
  downloads: number;
  dateModified: string;
  projectType: ContentKind;
  categories: string[];
  versions: string[];
}

export interface ContentSearchPage {
  hits: ContentProject[];
  offset: number;
  limit: number;
  totalHits: number;
}

export interface VersionDependency {
  versionId: string | null;
  projectId: string | null;
  fileName: string | null;
  dependencyType: "required" | "optional" | "incompatible" | "embedded";
}

export interface ContentVersion {
  id: string;
  projectId: string;
  name: string;
  versionNumber: string;
  versionType: "release" | "beta" | "alpha";
  datePublished: string;
  downloads: number;
  gameVersions: string[];
  loaders: string[];
  dependencies: VersionDependency[];
  files: Array<{
    hashes: Record<string, string>;
    url: string;
    filename: string;
    primary: boolean;
    size: number;
  }>;
}

export interface ManagedContent {
  id: string;
  kind: Exclude<ContentKind, "modpack">;
  source: "modrinth" | "local";
  projectId: string | null;
  versionId: string | null;
  versionNumber: string | null;
  name: string;
  fileName: string;
  sha1: string | null;
  sha512: string | null;
  size: number;
  enabled: boolean;
  managed: boolean;
  installedEpochMs: number;
  dependencies: string[];
  modMetadata: ModMetadata | null;
  diagnostics: string[];
  worldName: string | null;
}

export interface WorldInfo {
  name: string;
  datapackCount: number;
}

export interface ModMetadata {
  format: string;
  modIds: string[];
  name: string;
  version: string;
  description: string;
  authors: string[];
  dependencies: ModDependency[];
}

export interface ModDependency {
  id: string;
  requirement: string;
  mandatory: boolean;
}

export interface ContentUpdate {
  itemId: string;
  versionId: string;
  versionNumber: string;
}

export interface InstanceFileIssue {
  path: string;
  category: string;
  state: string;
  detail: string;
  repairable: boolean;
}

export interface InstanceFileReport {
  instanceId: string;
  scannedFiles: number;
  validFiles: number;
  missingFiles: number;
  corruptedFiles: number;
  issues: InstanceFileIssue[];
}

export interface RepairInstanceReport {
  before: InstanceFileReport;
  after: InstanceFileReport;
  repairedFiles: number;
}

export interface CrashFinding {
  code: string;
  severity: string;
  title: string;
  description: string;
  actions: string[];
  evidence: string[];
}

export interface CrashAnalysis {
  instanceId: string;
  status: "error" | "healthy" | "no-data" | "unknown";
  summary: string;
  sourceFiles: string[];
  findings: CrashFinding[];
  analyzedEpochMs: number;
}

export interface ExportInstanceReport {
  destination: string;
  files: number;
  bytes: number;
}

export interface InstallProgress {
  stage: "metadata" | "loader" | "libraries" | "assets" | "finalizing" | "complete";
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
