<script setup lang="ts">
defineProps<{ src: string; height?: number; title?: string }>()
</script>

<template>
  <div class="diagram-frame">
    <iframe
      :src="`/momo/diagrams/${src}`"
      :style="{ height: (height || 640) + 'px' }"
      :title="title || src"
      loading="lazy"
    />
    <a class="diagram-open" :href="`/momo/diagrams/${src}`" target="_blank" rel="noopener">
      在新窗口打开 ↗
    </a>
  </div>
</template>
