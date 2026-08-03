<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type {
  JavaRuntime,
  LauncherSettings,
  MemoryReport,
  SettingsSection,
  AppPaths,
  StoragePathSettings,
} from "../types";

const props = defineProps<{
  settings: LauncherSettings;
  section: SettingsSection;
  saving: boolean;
  error: string;
  javaRuntimes: JavaRuntime[];
  javaDetectionState: "loading" | "ready" | "unavailable";
  javaDownloadState: "idle" | "downloading";
  javaError: string;
  memoryReport: MemoryReport | null;
  memoryState: "loading" | "ready" | "unavailable";
  paths: AppPaths | null;
  storagePaths: StoragePathSettings;
}>();

const emit = defineEmits<{
  update: [settings: LauncherSettings];
  openDirectory: [target: "data" | "cache" | "logs" | "instances"];
  cleanDownloads: [];
  rescanJava: [];
  browseJava: [];
  installJava: [majorVersion: number];
  browseStorage: [target: "instances" | "cache"];
  resetStorage: [target: "instances" | "cache"];
}>();

const draft = ref<LauncherSettings>({ ...props.settings });
watch(
  () => props.settings,
  () => {
    draft.value = { ...props.settings };
  },
  { deep: true, immediate: true },
);
function update() {
  emit("update", { ...draft.value });
}

const selectedJavaIsKnown = computed(
  () =>
    !draft.value.preferredJavaPath ||
    props.javaRuntimes.some(
      (runtime) => runtime.path === draft.value.preferredJavaPath,
    ),
);
const memoryMaximumMb = computed(
  () => props.memoryReport?.maximumAssignableMb ?? 32768,
);
const memoryUsagePercent = computed(() => {
  const totalMb = (props.memoryReport?.totalBytes ?? 0) / (1024 * 1024);
  return totalMb
    ? Math.round((draft.value.defaultMemoryMb / totalMb) * 100)
    : 0;
});

function selectJava(event: Event) {
  draft.value.preferredJavaPath =
    (event.target as HTMLSelectElement).value || null;
  update();
}

function formatArchitecture(architecture?: string) {
  const normalized = architecture?.toLowerCase();
  if (normalized === "amd64" || normalized === "x86_64") return "x64";
  if (normalized === "aarch64" || normalized === "arm64") return "ARM64";
  if (normalized === "x86" || normalized === "i386") return "x86";
  return architecture || "架构未知";
}

function runtimeLabel(runtime: JavaRuntime) {
  const version = runtime.version ? `Java ${runtime.version}` : "Java";
  const vendor = runtime.vendor || runtime.source;
  return `${version} · ${formatArchitecture(runtime.architecture)} · ${vendor}`;
}

function formatMemory(megabytes: number) {
  const gigabytes = megabytes / 1024;
  return `${Number.isInteger(gigabytes) ? gigabytes : gigabytes.toFixed(1)} GB`;
}

function useRecommendedMemory() {
  if (!props.memoryReport) return;
  draft.value.defaultMemoryMb = props.memoryReport.recommendedMb;
  update();
}
</script>

<template>
  <div class="inline-settings-panel">
    <template v-if="section === 'general'">
      <label class="setting-control">
        <span><strong>默认打开页面</strong><small>NaCL 启动后首先显示的页面</small></span>
        <select v-model="draft.defaultPage" :disabled="saving" @change="update">
          <option value="home">首页</option>
          <option value="instances">实例</option>
          <option value="downloads">下载</option>
        </select>
      </label>
      <label class="setting-control">
        <span><strong>记住上次实例</strong><small>下次打开时继续选中当前实例</small></span>
        <input v-model="draft.rememberLastInstance" type="checkbox" :disabled="saving" @change="update" />
      </label>
      <label class="setting-control">
        <span><strong>关闭主窗口</strong><small>决定右上角关闭按钮的行为</small></span>
        <select v-model="draft.closeBehavior" :disabled="saving" @change="update">
          <option value="exit">退出 NaCL</option>
          <option value="minimize">最小化窗口</option>
        </select>
      </label>
      <label class="setting-control">
        <span><strong>自动检查更新</strong><small>启动时检查 NaCL 新版本</small></span>
        <input v-model="draft.checkUpdates" type="checkbox" :disabled="saving" @change="update" />
      </label>
      <label class="setting-control">
        <span><strong>系统通知</strong><small>安装完成或启动失败时提醒</small></span>
        <input v-model="draft.notifications" type="checkbox" :disabled="saving" @change="update" />
      </label>
    </template>

    <template v-else-if="section === 'appearance'">
      <label class="setting-control">
        <span><strong>界面主题</strong><small>保留冷青色强调色</small></span>
        <select v-model="draft.theme" :disabled="saving" @change="update">
          <option value="dark">深色</option>
          <option value="light">浅色</option>
        </select>
      </label>
      <label class="setting-control">
        <span><strong>界面密度</strong><small>调整列表和设置项的垂直间距</small></span>
        <select v-model="draft.interfaceDensity" :disabled="saving" @change="update">
          <option value="comfortable">舒适</option>
          <option value="compact">紧凑</option>
        </select>
      </label>
      <label class="setting-control">
        <span><strong>页面切换动画</strong><small>只使用低开销的位移和透明度</small></span>
        <input v-model="draft.animationsEnabled" type="checkbox" :disabled="saving" @change="update" />
      </label>
      <label v-if="draft.animationsEnabled" class="setting-control">
        <span><strong>动画速度</strong><small>影响页面切换和内容淡入</small></span>
        <select v-model="draft.animationSpeed" :disabled="saving" @change="update">
          <option value="fast">快速</option>
          <option value="normal">标准</option>
          <option value="relaxed">舒缓</option>
        </select>
      </label>
      <label class="setting-control">
        <span><strong>标题使用得意黑</strong><small>正文继续使用系统 UI 字体</small></span>
        <input v-model="draft.useSmileyHeadings" type="checkbox" :disabled="saving" @change="update" />
      </label>
    </template>

    <template v-else-if="section === 'java'">
      <label class="setting-control">
        <span><strong>自动检测 Java</strong><small>从系统与常见安装目录寻找运行环境</small></span>
        <input v-model="draft.autoDetectJava" type="checkbox" :disabled="saving" @change="update" />
      </label>
      <div class="setting-control setting-control--tall">
        <span>
          <strong>Java 选择</strong>
          <small>
            {{
              javaDetectionState === "loading"
                ? "正在实际启动并验证 Java…"
                : javaDetectionState === "unavailable"
                  ? "桌面端 Java 检测暂不可用"
                  : `已验证 ${javaRuntimes.length} 个运行环境`
            }}
          </small>
        </span>
        <div class="java-picker">
          <select
            :value="draft.preferredJavaPath ?? ''"
            :disabled="saving || javaDetectionState === 'loading'"
            @change="selectJava"
          >
            <option value="">自动选择（按游戏版本匹配）</option>
            <option
              v-if="!selectedJavaIsKnown && draft.preferredJavaPath"
              :value="draft.preferredJavaPath"
            >
              当前手动路径
            </option>
            <option
              v-for="runtime in javaRuntimes"
              :key="runtime.path"
              :value="runtime.path"
            >
              {{ runtimeLabel(runtime) }}
            </option>
          </select>
          <div class="java-picker-actions">
            <button
              class="setting-action"
              :disabled="saving || javaDetectionState === 'loading'"
              @click="emit('rescanJava')"
            >
              重新扫描
            </button>
            <button
              class="setting-action"
              :disabled="saving"
              @click="emit('browseJava')"
            >
              浏览 java.exe
            </button>
          </div>
        </div>
      </div>
      <label class="setting-control">
        <span><strong>管理独立运行环境</strong><small>缺少兼容 Java 时允许 NaCL 下载</small></span>
        <input v-model="draft.manageRuntimes" type="checkbox" :disabled="saving" @change="update" />
      </label>
      <div v-if="draft.manageRuntimes" class="setting-control setting-control--tall">
        <span>
          <strong>NaCL 托管运行环境</strong>
          <small>从 Eclipse Temurin 下载并校验，只保存在 NaCL 本地目录</small>
        </span>
        <div class="managed-runtime-options">
          <button
            v-for="major in [8, 17, 21, 25]"
            :key="major"
            class="runtime-install-button"
            :disabled="saving || javaDownloadState === 'downloading'"
            @click="emit('installJava', major)"
          >
            <span>Java {{ major }}</span>
            <small>
              {{
                javaRuntimes.some(
                  (runtime) => runtime.managed && runtime.majorVersion === major,
                )
                  ? "已安装"
                  : javaDownloadState === "downloading"
                    ? "请稍候"
                    : "下载"
              }}
            </small>
          </button>
        </div>
      </div>
      <label class="setting-control">
        <span><strong>兼容性提醒</strong><small>版本或位数不匹配时显示警告</small></span>
        <input v-model="draft.compatibilityWarnings" type="checkbox" :disabled="saving" @change="update" />
      </label>
    </template>

    <template v-else-if="section === 'defaults'">
      <div class="setting-control setting-control--range">
        <span>
          <strong>默认最大内存</strong>
          <small>
            {{
              memoryState === "loading"
                ? "正在读取机器物理内存"
                : memoryState === "unavailable"
                  ? "无法读取机器内存，使用安全范围"
                  : `机器内存 ${formatMemory((memoryReport?.totalBytes ?? 0) / (1024 * 1024))} · 当前占比 ${memoryUsagePercent}%`
            }}
          </small>
        </span>
        <div
          class="memory-range"
          :class="{
            warning: memoryUsagePercent > 50,
            danger: memoryUsagePercent > 75,
          }"
        >
          <div class="memory-range-head">
            <strong>{{ formatMemory(draft.defaultMemoryMb) }}</strong>
            <button
              v-if="memoryReport"
              class="text-action"
              :disabled="saving"
              @click="useRecommendedMemory"
            >
              推荐 {{ formatMemory(memoryReport.recommendedMb) }}
            </button>
          </div>
          <input
            v-model.number="draft.defaultMemoryMb"
            type="range"
            min="1024"
            :max="memoryMaximumMb"
            step="512"
            :disabled="saving"
            @change="update"
          />
          <div class="memory-range-scale">
            <span>1 GB</span>
            <span>上限 {{ formatMemory(memoryMaximumMb) }}</span>
          </div>
        </div>
      </div>
      <label class="setting-control">
        <span><strong>默认窗口宽度</strong><small>支持 854–7680 像素</small></span>
        <input v-model.number="draft.defaultWindowWidth" type="number" min="854" max="7680" :disabled="saving" @change="update" />
      </label>
      <label class="setting-control">
        <span><strong>默认窗口高度</strong><small>支持 480–4320 像素</small></span>
        <input v-model.number="draft.defaultWindowHeight" type="number" min="480" max="4320" :disabled="saving" @change="update" />
      </label>
    </template>

    <template v-else-if="section === 'storage'">
      <div class="setting-control setting-control--tall">
        <span>
          <strong>游戏实例安装目录</strong>
          <small>{{ paths?.instancesDir ?? "正在读取目录…" }}</small>
        </span>
        <div class="java-picker-actions">
          <button class="setting-action" @click="emit('openDirectory', 'instances')">打开</button>
          <button class="setting-action" :disabled="saving" @click="emit('browseStorage', 'instances')">更改</button>
          <button v-if="storagePaths.instancesDirectory" class="setting-action" :disabled="saving" @click="emit('resetStorage', 'instances')">恢复默认</button>
        </div>
      </div>
      <div class="setting-control setting-control--tall">
        <span>
          <strong>共享缓存保存目录</strong>
          <small>{{ paths?.cacheDir ?? "正在读取目录…" }}</small>
        </span>
        <div class="java-picker-actions">
          <button class="setting-action" @click="emit('openDirectory', 'cache')">打开</button>
          <button class="setting-action" :disabled="saving" @click="emit('browseStorage', 'cache')">更改</button>
          <button v-if="storagePaths.cacheDirectory" class="setting-action" :disabled="saving" @click="emit('resetStorage', 'cache')">恢复默认</button>
        </div>
      </div>
      <div class="offline-profile-note">切换目录不会移动或删除旧文件。NaCL 会立即改用新位置；切回原目录后，旧实例和缓存仍可继续使用。</div>
      <div class="setting-control">
        <span><strong>临时下载</strong><small>清理由中断任务留下的 .part 与 .tmp 文件</small></span>
        <button class="setting-action" :disabled="saving" @click="emit('cleanDownloads')">清理</button>
      </div>
    </template>

    <template v-else>
      <label class="setting-control">
        <span><strong>日志级别</strong><small>调试级别会记录更多启动细节</small></span>
        <select v-model="draft.logLevel" :disabled="saving" @change="update">
          <option value="error">仅错误</option>
          <option value="warn">警告</option>
          <option value="info">信息</option>
          <option value="debug">调试</option>
        </select>
      </label>
      <label class="setting-control">
        <span><strong>日志保留天数</strong><small>范围 1–365 天</small></span>
        <input v-model.number="draft.logRetentionDays" type="number" min="1" max="365" :disabled="saving" @change="update" />
      </label>
      <label class="setting-control">
        <span><strong>失败后自动诊断</strong><small>记录 Java、实例和路径概况</small></span>
        <input v-model="draft.autoDiagnostics" type="checkbox" :disabled="saving" @change="update" />
      </label>
      <div class="setting-control">
        <span><strong>日志目录</strong><small>查看启动器产生的本地文本记录</small></span>
        <button class="setting-action" @click="emit('openDirectory', 'logs')">打开</button>
      </div>
    </template>

    <div class="inline-save-state" :class="{ error }">
      {{
        error ||
        (section === "java" ? javaError : "") ||
        (saving
          ? "正在保存…"
          : javaDownloadState === "downloading"
            ? "正在下载并校验 Java 运行环境…"
            : "修改后自动保存")
      }}
    </div>
  </div>
</template>
