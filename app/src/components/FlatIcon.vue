<script setup lang="ts">
export type FlatIconName =
  | "home"
  | "instances"
  | "download"
  | "file"
  | "help"
  | "settings"
  | "user"
  | "plus"
  | "trash"
  | "play"
  | "refresh"
  | "moon"
  | "sun"
  | "minimize"
  | "maximize"
  | "close";

defineProps<{
  name: FlatIconName;
}>();
</script>

<template>
  <svg
    class="flat-icon"
    :class="{ 'flat-icon--solid': name === 'play' }"
    viewBox="0 0 24 24"
    aria-hidden="true"
  >
    <template v-if="name === 'home'">
      <path d="M4 10.5 12 4l8 6.5v8A1.5 1.5 0 0 1 18.5 20h-13A1.5 1.5 0 0 1 4 18.5z" />
      <path d="M9.5 20v-6h5v6" />
    </template>
    <template v-else-if="name === 'instances'">
      <rect x="5" y="4" width="14" height="16" rx="2" />
      <path d="M8.5 8h7M8.5 12h7M8.5 16h4" />
    </template>
    <template v-else-if="name === 'download'">
      <path d="M12 3v12m-4.5-4L12 15.5l4.5-4.5M5 20h14" />
    </template>
    <template v-else-if="name === 'file'">
      <path d="M6 3h8l4 4v14H6zM14 3v5h5M9 13h6M9 17h6" />
    </template>
    <template v-else-if="name === 'help'">
      <circle cx="12" cy="12" r="9" />
      <path d="M9.8 9.1a2.4 2.4 0 1 1 3.3 2.2c-.8.4-1.1.8-1.1 1.7M12 17h.01" />
    </template>
    <template v-else-if="name === 'settings'">
      <circle cx="12" cy="12" r="3" />
      <path d="M19.3 15a1.7 1.7 0 0 0 .3 1.9l.1.1-2.7 2.7-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.6v.2h-4v-.2a1.7 1.7 0 0 0-1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1L4.3 17l.1-.1a1.7 1.7 0 0 0 .3-1.9A1.7 1.7 0 0 0 3.1 14H3v-4h.1a1.7 1.7 0 0 0 1.6-1 1.7 1.7 0 0 0-.3-1.9L4.3 7 7 4.3l.1.1a1.7 1.7 0 0 0 1.9.3 1.7 1.7 0 0 0 1-1.6V3h4v.1a1.7 1.7 0 0 0 1 1.6 1.7 1.7 0 0 0 1.9-.3l.1-.1L19.7 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.6 1h.1v4h-.1a1.7 1.7 0 0 0-1.6 1Z" />
    </template>
    <template v-else-if="name === 'user'">
      <circle cx="12" cy="8" r="3" />
      <path d="M6 19a6 6 0 0 1 12 0" />
    </template>
    <template v-else-if="name === 'plus'">
      <path d="M12 5v14M5 12h14" />
    </template>
    <template v-else-if="name === 'trash'">
      <path d="M4.5 7h15M9 4.5h6M7 7l.7 12h8.6L17 7M10 10v6M14 10v6" />
    </template>
    <template v-else-if="name === 'play'">
      <path d="m8 5 11 7-11 7z" />
    </template>
    <template v-else-if="name === 'refresh'">
      <path d="M19 7v5h-5" />
      <path d="M18.2 12A6.5 6.5 0 1 1 16 7.2L19 10" />
    </template>
    <template v-else-if="name === 'moon'">
      <path d="M20 15.5A8.5 8.5 0 0 1 8.5 4 8.5 8.5 0 1 0 20 15.5Z" />
    </template>
    <template v-else-if="name === 'sun'">
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
    </template>
    <template v-else-if="name === 'minimize'">
      <path d="M5 12h14" />
    </template>
    <template v-else-if="name === 'maximize'">
      <rect x="5" y="5" width="14" height="14" rx="1" />
    </template>
    <template v-else-if="name === 'close'">
      <path d="m6 6 12 12M18 6 6 18" />
    </template>
  </svg>
</template>
