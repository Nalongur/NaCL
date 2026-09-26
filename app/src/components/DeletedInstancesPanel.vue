<script setup lang="ts">
import type { LauncherInstance } from "../types";

defineProps<{
  instances: LauncherInstance[];
  busy: boolean;
  error: string;
}>();

const emit = defineEmits<{
  restore: [instanceId: string];
  remove: [instanceId: string];
}>();
</script>

<template>
  <section class="deleted-instances" aria-label="已删除实例">
    <div class="deleted-instances-heading">
      <div>
        <div class="eyebrow">实例回收区</div>
        <h2>已删除实例</h2>
      </div>
      <p>实例和存档仍保存在当前实例目录的 .deleted 文件夹中。</p>
    </div>
    <div v-if="error" class="inline-error">{{ error }}</div>
    <div v-if="instances.length === 0" class="data-empty">回收区为空。</div>
    <div v-for="instance in instances" :key="instance.id" class="deleted-instance-row">
      <div>
        <div class="setting-name">{{ instance.name }}</div>
        <div class="setting-description">Minecraft {{ instance.gameVersion }}</div>
      </div>
      <div class="deleted-instance-actions">
        <button class="setting-action" :disabled="busy" @click="emit('restore', instance.id)">恢复</button>
        <button class="setting-action danger-action" :disabled="busy" @click="emit('remove', instance.id)">彻底删除</button>
      </div>
    </div>
  </section>
</template>
