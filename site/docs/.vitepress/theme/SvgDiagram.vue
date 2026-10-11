<script setup>
import { ref, watchEffect } from 'vue'
import { useData, withBase } from 'vitepress'

const props = defineProps({
  src: { type: String, required: true },
  title: { type: String, default: '' },
})
const { isDark } = useData()
const html = ref('')
watchEffect(async () => {
  isDark.value
  const res = await fetch(withBase(`/diagrams/${props.src}`))
  html.value = await res.text()
})
</script>

<template>
  <figure class="svg-diagram">
    <a class="svg-open" :href="withBase(`/diagrams/${src}`)" target="_blank" rel="noopener">在新窗口打开 ⤴</a>
    <div class="svg-body" v-html="html" role="img" :aria-label="title || src" />
  </figure>
</template>
