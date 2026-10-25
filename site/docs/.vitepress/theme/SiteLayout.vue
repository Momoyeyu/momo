<script setup lang="ts">
import { useData, useRouter, withBase } from 'vitepress'
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import ProductHome from './ProductHome.vue'
import HelpPage from './HelpPage.vue'
import SiteHeader from './SiteHeader.vue'
import { copy } from './product-copy.mjs'
import './page-switch.css'
const { page } = useData()
const router = useRouter()
const home = computed(() => /^(?:(?:en|ja)\/)?index\.md$/.test(page.value.relativePath))
const guideLocale = ref<'zh' | 'en' | 'ja'>('zh')
const locale = computed<'zh' | 'en' | 'ja'>(() => home.value ? page.value.relativePath.startsWith('en/') ? 'en' : page.value.relativePath.startsWith('ja/') ? 'ja' : 'zh' : guideLocale.value)
const skipLink = ref<HTMLAnchorElement>()
const view = ref<HTMLElement>()
const switching = ref(false)
let transition: ViewTransition | undefined
let enterAnimation: Animation | undefined
let navigation = 0
let bypass = false
let mounted = false
const previousBeforeRoute = router.onBeforeRouteChange

function tabFor(href: string) {
  const url = new URL(href, location.href)
  if (url.origin !== location.origin || !url.pathname.startsWith(withBase('/'))) return -1
  const path = url.pathname.slice(withBase('/').length).replace(/\.html$/, '').replace(/\/$/, '')
  if (/^(?:(?:en|ja)(?:\/index)?|index)?$/.test(path)) return 0
  if (/^help\/(?:getting-started|voice|data)$/.test(path)) return 1
  return -1
}
async function navigate(href: string, scrollPosition?: number) {
  const id = ++navigation
  const target = tabFor(href), currentTab = home.value ? 0 : 1
  const direction = target > currentTab ? 'forward' : 'backward'
  transition?.skipTransition()
  enterAnimation?.cancel()
  transition = undefined; enterAnimation = undefined
  switching.value = true
  document.documentElement.dataset.pageSwitch = direction
  const update = async () => {
    if (!mounted || id !== navigation) return
    bypass = true
    await router.go(href)
    await nextTick()
    if (scrollPosition !== undefined) {
      // VitePress defers hash navigation to the next frame. History owns the
      // reading position on a back/forward visit, so restore after that work.
      requestAnimationFrame(() => {
        if (mounted && id === navigation) window.scrollTo({ top: scrollPosition, behavior: 'instant' as ScrollBehavior })
      })
    }
  }
  if (target !== currentTab && document.startViewTransition && !matchMedia('(prefers-reduced-motion: reduce)').matches) {
    const current = transition = document.startViewTransition(update)
    // Cancellation/rapid navigation is a normal outcome, not an unhandled rejection.
    void current.ready.catch(() => {})
    try { await current.finished } catch { /* Router retains its normal failure handling. */ }
    finally { if (id === navigation) { transition = undefined; switching.value = false; delete document.documentElement.dataset.pageSwitch } }
  } else {
    try {
      await update()
      if (target !== currentTab && id === navigation && !matchMedia('(prefers-reduced-motion: reduce)').matches) {
        enterAnimation = view.value?.animate([{ transform: `translateX(${direction === 'forward' ? '' : '-'}100vw)` }, { transform: 'translateX(0)' }], { duration: 350, easing: 'cubic-bezier(0.65, 0, 0.35, 1)' })
        await enterAnimation?.finished.catch(() => {})
      }
    } finally { if (id === navigation) { switching.value = false; delete document.documentElement.dataset.pageSwitch } }
  }
}
function beforeRoute(href: string) {
  if (bypass) { bypass = false; return previousBeforeRoute?.(href) }
  const target = tabFor(href)
  if (target < 0 || (target === (home.value ? 0 : 1) && !switching.value)) return previousBeforeRoute?.(href)
  void navigate(href)
  return false
}
function repeatTab(event: MouseEvent) {
  if (!switching.value || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return
  const link = (event.target as Element).closest<HTMLAnchorElement>('.primary-nav a[href]')
  // VitePress handles a link to the current URL without entering its route hook.
  // It must still cancel a pending switch when the player selects that tab again.
  if (link?.href === location.href && tabFor(link.href) === (home.value ? 0 : 1)) void navigate(link.href)
}
function pop(event: PopStateEvent) {
  const target = tabFor(location.href)
  if (!event.state || target < 0 || (target === (home.value ? 0 : 1) && !switching.value)) return
  event.stopImmediatePropagation()
  void navigate(location.href, event.state.scrollPosition || 0)
}
function clearSkipFocus(event: PointerEvent) {
  if (document.activeElement === skipLink.value && event.target !== skipLink.value) skipLink.value?.blur()
}
onMounted(() => {
  mounted = true
  const selected = new URL(location.href).searchParams.get('lang')
  if (selected === 'en' || selected === 'ja') guideLocale.value = selected
  router.onBeforeRouteChange = beforeRoute
  window.addEventListener('click', repeatTab, true)
  window.addEventListener('popstate', pop, true)
  document.addEventListener('pointerdown', clearSkipFocus, true)
})
onBeforeUnmount(() => {
  mounted = false
  navigation++
  transition?.skipTransition(); enterAnimation?.cancel()
  delete document.documentElement.dataset.pageSwitch
  if (router.onBeforeRouteChange === beforeRoute) router.onBeforeRouteChange = previousBeforeRoute
  window.removeEventListener('click', repeatTab, true)
  window.removeEventListener('popstate', pop, true)
  document.removeEventListener('pointerdown', clearSkipFocus, true)
})
</script>
<template>
  <a ref="skipLink" class="skip-link" :href="home ? '#main' : '#help-content'">{{ copy[locale].skip }}</a>
  <SiteHeader :locale="locale" :help="!home" />
  <div class="site-view-clip"><div ref="view" class="site-route-view" :inert="switching" data-testid="site-route-view"><ProductHome v-if="home" /><HelpPage v-else @locale-change="guideLocale = $event" /></div></div>
</template>
