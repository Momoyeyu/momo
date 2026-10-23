<script setup lang="ts">
import { Content, useData, useRouter, withBase } from 'vitepress'
import { helpLinks } from './product-content.mjs'
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import SiteHeader from './SiteHeader.vue'
import { copy, localePath } from './product-copy.mjs'
const { page } = useData()
const locale = ref<'zh' | 'en' | 'ja'>('zh')
const t = computed(() => copy[locale.value])
const labels = computed(() => ({
  zh: { home: '首页', guides: '使用帮助', breadcrumb: '当前位置', index: '从这里开始', part: '指南' },
  en: { home: 'Home', guides: 'Guides', breadcrumb: 'Breadcrumb', index: 'Start here', part: 'Guide' },
  ja: { home: 'ホーム', guides: '使い方', breadcrumb: '現在の位置', index: 'ここからはじめる', part: 'ガイド' },
}[locale.value]))
const guideEntries = computed(() => Object.values(helpLinks).map((href, index) => ({
  href,
  number: String(index + 1).padStart(2, '0'),
  title: t.value.helpNav[index],
  current: page.value.relativePath === `${href.slice(1)}.md`,
})))
const currentGuide = computed(() => guideEntries.value.find(guide => guide.current))
const localizedHref = (path: string) => withBase(path) + (locale.value === 'zh' ? '' : `?lang=${locale.value}`)
const router = useRouter()
const previousAfterRoute = router.onAfterRouteChange
function syncGuideLanguage(address = window.location.href) {
  const selected = new URL(address, window.location.href).searchParams.get('lang')
  locale.value = selected === 'en' || selected === 'ja' ? selected : 'zh'
  for (const link of document.querySelectorAll<HTMLAnchorElement>('.help-page article a[href]')) {
    const url = new URL(link.href)
    if (url.origin !== window.location.origin || !url.pathname.startsWith(withBase('/help/'))) continue
    if (locale.value === 'zh') url.searchParams.delete('lang')
    else url.searchParams.set('lang', locale.value)
    link.href = url.href
  }
}
const afterRoute = async (address: string) => {
  await previousAfterRoute?.(address)
  await nextTick()
  syncGuideLanguage(address)
}
const onPopState = () => { void nextTick(() => syncGuideLanguage()) }
onMounted(() => {
  syncGuideLanguage()
  router.onAfterRouteChange = afterRoute
  window.addEventListener('popstate', onPopState)
})
onBeforeUnmount(() => {
  if (router.onAfterRouteChange === afterRoute) router.onAfterRouteChange = previousAfterRoute
  window.removeEventListener('popstate', onPopState)
})
</script>
<template>
  <a class="skip-link" href="#help-content">{{ t.skip }}</a>
  <SiteHeader :locale="locale" help />
  <main id="help-content" class="help-page" tabindex="-1">
    <div class="help-atmosphere" aria-hidden="true"><img :src="withBase('/brand/momo-presence-v2.webp')" alt=""></div>
    <div class="help-shell">
      <nav class="help-breadcrumb" :aria-label="labels.breadcrumb">
        <a :href="withBase(localePath(locale))">{{ labels.home }}</a><span aria-hidden="true">/</span>
        <a :href="localizedHref(helpLinks.start)">{{ labels.guides }}</a>
        <template v-if="currentGuide"><span aria-hidden="true">/</span><span aria-current="page">{{ currentGuide.title }}</span></template>
      </nav>
      <aside class="help-index">
        <p class="help-index-title">{{ labels.index }}</p>
        <nav class="guide-navigation" :aria-label="labels.guides" data-testid="guide-navigation">
          <a v-for="guide in guideEntries" :key="guide.href" :href="localizedHref(guide.href)" :aria-current="guide.current ? 'page' : undefined">
            <span class="guide-number" aria-hidden="true">{{ guide.number }}</span><span>{{ guide.title }}</span>
          </a>
        </nav>
        <a class="back-home" :href="withBase(localePath(locale))"><span aria-hidden="true">↶</span>{{ t.back }}</a>
      </aside>
      <div class="help-reading">
        <div v-if="currentGuide" class="guide-folio" aria-hidden="true"><span>{{ labels.part }} {{ currentGuide.number }}</span><i></i><span>momo</span></div>
        <p v-if="t.helpNote" class="translation-note">{{ t.helpNote }}</p>
        <article v-if="!page.isNotFound" lang="zh-CN"><Content /></article>
        <article v-else><h1>{{ t.missing }}</h1><p><a :href="withBase(localePath(locale))">{{ t.back }}</a></p></article>
      </div>
    </div>
  </main>
  <footer class="help-footer">momo · {{ t.tagline }}</footer>
</template>
