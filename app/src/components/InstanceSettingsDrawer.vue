<script setup lang="ts">
import { ref, watch } from "vue";
import type { InstanceTab, LauncherInstance } from "../types";
import FlatIcon from "./FlatIcon.vue";

const props = defineProps<{
  open: boolean;
  instance: LauncherInstance | null;
  tab: InstanceTab;
  saving: boolean;
  error: string;
}>();

const emit = defineEmits<{
  close: [];
  save: [instance: LauncherInstance];
  openDirectory: [];
  browseVersions: [];
}>();

const draft = ref<LauncherInstance | null>(null);

watch(
  () => [props.open, props.instance] as const,
  () => {
    draft.value = props.instance
      ? JSON.parse(JSON.stringify(props.instance)) as LauncherInstance
      : null;
  },
  { deep: true, immediate: true },
);

function setJavaMode(event: Event) {
  if (!draft.value) return;
  const mode = (event.target as HTMLSelectElement).value;
  draft.value.java =
    mode === "custom" ? { mode: "custom", path: "" } : { mode: "auto" };
}

function save() {
  if (draft.value) emit("save", draft.value);
}
</script>

<template>
  <Transition name="drawer">
    <div
      v-if="open && draft"
      class="drawer-layer"
      role="presentation"
      @mousedown.self="emit('close')"
    >
      <aside class="create-drawer" aria-labelledby="instance-settings-title">
        <header class="drawer-header">
          <div>
            <div class="drawer-brand">NaCL · {{ draft.name }}</div>
            <h2 id="instance-settings-title">编辑实例</h2>
          </div>
          <button class="icon-button" aria-label="关闭实例设置" @click="emit('close')">
            <FlatIcon name="close" />
          </button>
        </header>

        <form class="create-form" @submit.prevent="save">
          <div class="drawer-scroll">
            <template v-if="tab === 'overview'">
              <label class="field">
                <span class="field-label">实例名称</span>
                <input v-model="draft.name" maxlength="64" :disabled="saving" />
              </label>
              <div class="drawer-summary">
                <div><span>加载器</span><strong>原版 Vanilla</strong></div>
                <div><span>实例 ID</span><strong class="mono-value">{{ draft.id }}</strong></div>
              </div>
            </template>

            <template v-else-if="tab === 'version'">
              <div class="drawer-summary">
                <div><span>当前版本</span><strong>{{ draft.gameVersion }}</strong></div>
                <div><span>加载器</span><strong>原版 Vanilla</strong></div>
              </div>
              <div class="form-intro">切换游戏版本需要重新解析客户端、依赖库与资源文件，因此统一从下载页执行安装。</div>
              <button type="button" class="secondary-button wide-button" @click="emit('browseVersions')">前往版本下载</button>
            </template>

            <template v-else-if="tab === 'runtime'">
              <label class="field">
                <span class="field-label">Java 选择方式</span>
                <select :value="draft.java.mode" :disabled="saving" @change="setJavaMode">
                  <option value="auto">自动选择</option>
                  <option value="custom">自定义路径</option>
                </select>
              </label>
              <label v-if="draft.java.mode === 'custom'" class="field">
                <span class="field-label">java.exe 路径</span>
                <input v-model="draft.java.path" :disabled="saving" />
              </label>
              <div class="field-grid">
                <label class="field">
                  <span class="field-label">最小内存（MB）</span>
                  <input
                    v-model.number="draft.memory.minimumMb"
                    type="number"
                    min="512"
                    max="32768"
                    step="512"
                    :disabled="saving"
                  />
                </label>
                <label class="field">
                  <span class="field-label">最大内存（MB）</span>
                  <input
                    v-model.number="draft.memory.maximumMb"
                    type="number"
                    min="512"
                    max="32768"
                    step="512"
                    :disabled="saving"
                  />
                </label>
              </div>
              <label class="field">
                <span class="field-label">附加 JVM 参数</span>
                <textarea
                  v-model="draft.advanced.jvmArguments"
                  maxlength="4096"
                  :disabled="saving"
                  placeholder="-XX:+UseG1GC"
                ></textarea>
              </label>
            </template>

            <template v-else-if="tab === 'display'">
              <label class="field">
                <span class="field-label">启动模式</span>
                <select v-model="draft.display.mode" :disabled="saving">
                  <option value="windowed">窗口</option>
                  <option value="maximized">最大化</option>
                  <option value="fullscreen">全屏</option>
                </select>
              </label>
              <div class="field-grid">
                <label class="field">
                  <span class="field-label">宽度</span>
                  <input
                    v-model.number="draft.display.width"
                    type="number"
                    min="854"
                    max="7680"
                    :disabled="saving"
                  />
                </label>
                <label class="field">
                  <span class="field-label">高度</span>
                  <input
                    v-model.number="draft.display.height"
                    type="number"
                    min="480"
                    max="4320"
                    :disabled="saving"
                  />
                </label>
              </div>
            </template>

            <template v-else-if="tab === 'files'">
              <div class="form-intro">实例目录只包含该实例的存档、截图、资源包和配置。</div>
              <button type="button" class="secondary-button wide-button" @click="emit('openDirectory')">
                打开实例目录
              </button>
            </template>

            <template v-else>
              <label class="field">
                <span class="field-label">附加游戏参数</span>
                <textarea
                  v-model="draft.advanced.gameArguments"
                  maxlength="4096"
                  :disabled="saving"
                ></textarea>
              </label>
              <label class="switch-row">
                <span>
                  <strong>调试日志</strong>
                  <small>记录更详细的启动与游戏输出</small>
                </span>
                <input
                  v-model="draft.advanced.debugLogging"
                  type="checkbox"
                  :disabled="saving"
                />
              </label>
            </template>

            <div v-if="error" class="form-error">{{ error }}</div>
          </div>

          <div v-if="tab !== 'files' && tab !== 'version'" class="drawer-actions">
            <button type="button" class="secondary-button" :disabled="saving" @click="emit('close')">
              取消
            </button>
            <button type="submit" class="primary-button" :disabled="saving">
              {{ saving ? "正在保存…" : "保存设置" }}
            </button>
          </div>
        </form>
      </aside>
    </div>
  </Transition>
</template>
