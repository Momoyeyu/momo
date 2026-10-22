<script setup lang="ts">
import { Content, useData, useRouter, withBase } from 'vitepress'
import { helpLinks } from './product-content.mjs'
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import SiteHeader from './SiteHeader.vue'
import { copy, localePath } from './product-copy.mjs'
const { page } = useData()
const locale = ref<'zh' | 'en' | 'ja'>('zh')
const t = computed(() => copy[locale.value])
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
  <main id="help-content" class="help-page">
    <a class="back-home" :href="withBase(localePath(locale))">{{ t.back }}</a>
    <p v-if="t.helpNote" class="translation-note">{{ t.helpNote }}</p>
    <nav :aria-label="t.nav[3]"><a v-for="(href, key) in helpLinks" :key="key" :href="localizedHref(href)" :aria-current="page.relativePath === `${href.slice(1)}.md` ? 'page' : undefined">{{ t.helpNav[{ start: 0, voice: 1, data: 2 }[key]] }}</a></nav>
    <article v-if="!page.isNotFound"><Content /></article>
    <article v-else><h1>{{ t.missing }}</h1><p><a :href="withBase(localePath(locale))">{{ t.back }}</a></p></article>
  </main>
  <footer class="help-footer">momo · {{ t.tagline }}</footer>
</template>
<style scoped>
.back-home{display:inline-block;font-size:13px;color:var(--accent);margin-bottom:25px}.translation-note{font-size:13px;color:var(--secondary);border-left:1px solid var(--accent);padding:12px 18px;margin:0 0 28px;background:var(--panel);line-height:1.9}
.help-page{max-width:960px;padding-top:48px;padding-bottom:80px}
.help-page nav{gap:22px;flex-wrap:wrap;border-bottom:1px solid var(--border);padding-bottom:20px;margin-bottom:42px}
.help-page nav a{display:inline!important;font-size:14px;border:0;padding:0}
.help-page nav a[aria-current]{color:var(--accent)}
.help-page article{max-width:760px}
.help-page :deep(h1){font-size:clamp(27px,4vw,38px);line-height:1.5;margin:0 0 30px;font-weight:500}
.help-page :deep(h2){font-size:23px;margin:40px 0 18px}
.help-page :deep(h3){font-size:19px;margin:30px 0 16px}
.help-page :deep(p),.help-page :deep(ul),.help-page :deep(ol){margin:18px 0;line-height:1.95;color:var(--secondary);font-size:15px}
.help-page :deep(li){margin:8px 0}
.help-page :deep(a){color:var(--accent);text-decoration:underline;text-underline-offset:4px;overflow-wrap:anywhere}
.help-page :deep(.header-anchor){display:none}
.help-page :deep(pre){overflow:auto;max-width:100%;padding:20px;background:var(--panel);border:1px solid var(--border);border-radius:12px;line-height:1.8}
.help-page :deep(code){font-size:.88em;overflow-wrap:anywhere}
.help-page :deep(table){display:block;overflow-x:auto;border-collapse:collapse;max-width:100%;font-size:14px;margin:24px 0}
.help-page :deep(td),.help-page :deep(th){padding:12px 16px;border:1px solid var(--border);text-align:left}
.help-page :deep(.vp-code-lang){display:none}
.help-page :deep(button.copy){display:none}
.help-footer{max-width:864px;margin:auto;padding:24px 0 40px;border-top:1px solid var(--border);font-size:12px;color:var(--secondary)}
@media(max-width:700px){.help-page{padding-top:25px;padding-bottom:50px}.help-page nav{gap:18px}.help-footer{margin:0 6%}}
</style>
