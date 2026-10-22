<script setup lang="ts">
import { withBase } from 'vitepress'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { copy, languages, localePath } from './product-copy.mjs'
import { helpLinks } from './product-content.mjs'
const props = defineProps<{ locale: 'zh' | 'en' | 'ja'; help?: boolean }>()
const menu = ref<HTMLDetailsElement>()
const summary = ref<HTMLElement>()
const t = computed(() => copy[props.locale])
const currentHash = ref('')
function syncHash() { currentHash.value = window.location.hash }
function selectLanguage() { if (menu.value) menu.value.open = false }
function closeOutside(event: PointerEvent) {
  if (menu.value?.open && !menu.value.contains(event.target as Node)) menu.value.open = false
}
function menuKey(event: KeyboardEvent) {
  if (event.key === 'Escape' && menu.value?.open) {
    event.preventDefault()
    menu.value.open = false
    summary.value?.focus()
  }
}
onMounted(() => {
  syncHash()
  window.addEventListener('hashchange', syncHash)
  document.addEventListener('pointerdown', closeOutside)
})
onBeforeUnmount(() => {
  window.removeEventListener('hashchange', syncHash)
  document.removeEventListener('pointerdown', closeOutside)
})
</script>
<template>
  <header class="site-header">
    <a class="wordmark" :href="withBase(localePath(locale))" :aria-label="t.home"><img class="brand-mark" :src="withBase('/brand/momo-mark.svg')" alt=""><img class="brand-wordmark" :src="withBase('/brand/momo-wordmark.svg')" alt="momo"></a>
    <span class="header-thread" aria-hidden="true"><i></i></span>
    <nav class="primary-nav" :aria-label="t.nav[0]">
      <template v-if="!help"><a href="#together">{{ t.nav[0] }}</a><a href="#memory">{{ t.nav[1] }}</a><a href="#faq">{{ t.nav[2] }}</a></template>
      <a :href="withBase(helpLinks.start) + (locale === 'zh' ? '' : `?lang=${locale}`)">{{ t.nav[3] }}</a>
    </nav>
    <div class="header-utilities">
      <details ref="menu" class="language-menu" @keydown="menuKey" data-testid="language-menu"><summary ref="summary" :aria-label="t.language"><svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="1.3" aria-hidden="true"><circle cx="12" cy="12" r="9"/><ellipse cx="12" cy="12" rx="4" ry="9"/><path d="M3 12h18M5 6h14M5 18h14"/></svg><span>{{ languages[locale] }}</span><span class="language-chevron" aria-hidden="true">⌄</span></summary><ul><li v-for="(label, code) in languages" :key="code"><a :href="withBase(localePath(code)) + (help ? '' : currentHash)" :lang="copy[code].lang" :hreflang="copy[code].lang" :aria-current="locale === code ? 'true' : undefined" :data-testid="`locale-${code}`" @click="selectLanguage">{{ label }}<span v-if="locale === code" aria-hidden="true">✧</span></a></li></ul></details>
      <a class="github-link" href="https://github.com/Momoyeyu/momo" :aria-label="t.github" :title="t.github"><svg viewBox="0 0 24 24" width="21" height="21" fill="currentColor" aria-hidden="true"><path d="M12 .75a11.25 11.25 0 0 0-3.558 21.924c.563.104.769-.244.769-.542 0-.267-.01-.974-.016-1.912-3.13.68-3.79-1.509-3.79-1.509-.512-1.3-1.25-1.646-1.25-1.646-1.023-.7.077-.686.077-.686 1.131.08 1.727 1.16 1.727 1.16 1.005 1.723 2.637 1.225 3.279.937.102-.728.393-1.225.715-1.507-2.499-.284-5.126-1.25-5.126-5.565 0-1.23.44-2.234 1.16-3.022-.117-.285-.503-1.43.11-2.98 0 0 .945-.303 3.094 1.154A10.78 10.78 0 0 1 12 6.177a10.8 10.8 0 0 1 2.815.379c2.148-1.457 3.09-1.153 3.09-1.153.615 1.549.229 2.694.112 2.979.722.788 1.158 1.793 1.158 3.022 0 4.327-2.631 5.278-5.138 5.557.404.348.764 1.034.764 2.084 0 1.504-.014 2.718-.014 3.087 0 .3.203.65.774.54A11.25 11.25 0 0 0 12 .75Z"/></svg></a>
    </div>
  </header>
</template>
