<script setup lang="ts">
import { useData, withBase } from 'vitepress'
import SiteHeader from './SiteHeader.vue'
import ProductChapters from './ProductChapters.vue'
import { copy, localePath } from './product-copy.mjs'
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { helpLinks, releaseUrl } from './product-content.mjs'
import { clamp, meetingFrame } from './meeting-motion.mjs'
import { createMeetingPixels } from './meeting-pixels.mjs'
import { createScrollFollow } from './scroll-follow.mjs'

const creditsOpen = ref(false)
const skipLink = ref<HTMLAnchorElement>()
const active = ref(0)
const { page } = useData()
const locale = computed<'zh' | 'en' | 'ja'>(() => page.value.relativePath.startsWith('en/') ? 'en' : page.value.relativePath.startsWith('ja/') ? 'ja' : 'zh')
const t = computed(() => copy[locale.value])
const themes = computed(() => t.value.themes)
const helpHref = (key: keyof typeof helpLinks) => withBase(helpLinks[key]) + (locale.value === 'zh' ? '' : `?lang=${locale.value}`)
const deck = ref<HTMLElement>()
const runway = ref<HTMLElement>()
const scene = ref<HTMLElement>()
const pixelCanvas = ref<HTMLCanvasElement>()
const presenceFigure = ref<HTMLImageElement>()
let pixels: Awaited<ReturnType<typeof createMeetingPixels>> | undefined
let sceneLoading = false
let sceneFailed = false
let initialization = 0
const animated = ref(false)
const sceneVisible = ref(true)
const pageHidden = ref(false)
const sceneProgress = ref(0)
const sceneCopy = computed(() => ({
  zh: { skip: '直接看真实对话', scroll: '向下，走近一点', title: '先从一句「你好」开始。', body: '打字，或打开麦克风。她用表情和动作回应，聊过的小事，也可以留到下次。' },
  en: { skip: 'Go to the conversation', scroll: 'Scroll a little closer', title: 'Start with a simple hello.', body: 'Type, or speak into your microphone. See her respond through expressions and motion. Keep little things for your next conversation.' },
  ja: { skip: '会話を見る', scroll: 'スクロールして、もう少し近くへ', title: 'まずは、「こんにちは」から。', body: '文字でも、マイクでも。表情や動きで返事が届き、話した小さなことを、次の会話へ残せます。' },
}[locale.value]))
let sceneObserver: IntersectionObserver | undefined
let resizeObserver: ResizeObserver | undefined
let layoutRequest = 0
let hashRequest = 0
function readSceneProgress() {
  if (!scene.value || !runway.value) return sceneProgress.value
  const rect = runway.value.getBoundingClientRect()
  return clamp(-rect.top / Math.max(1, runway.value.offsetHeight - scene.value.offsetHeight))
}
function paintScene(progress: number) {
  if (!scene.value || !deck.value || !animated.value) return
  const wasRevealed = sceneProgress.value >= .7
  sceneProgress.value = progress
  if (progress < .97 && active.value !== 0) active.value = 0
  const frame = meetingFrame(progress, deck.value.getBoundingClientRect(), { width: innerWidth, height: innerHeight })
  const style = scene.value.style
  for (const [key, value] of Object.entries(frame)) style.setProperty(`--scene-${key}`, String(value))
  scene.value.dataset.progress = progress.toFixed(4)
  scene.value.dataset.landed = String(progress >= .97)
  if (pixels && pixelCanvas.value && presenceFigure.value && video.value) {
    const origin = scene.value.getBoundingClientRect()
    const relative = (rect: DOMRect) => ({ left: rect.left - origin.left, top: rect.top - origin.top, width: rect.width, height: rect.height })
    pixels.draw(progress, { width: origin.width, height: origin.height }, relative(presenceFigure.value.getBoundingClientRect()), relative(video.value.getBoundingClientRect()))
  }
  // The displayed phase owns playback too: clear the canvas before starting the live surface.
  if (wasRevealed !== (progress >= .7)) void syncPlayback()
}
const sceneFollow = createScrollFollow({ read: readSceneProgress, write: paintScene })
function requestScene() {
  if (animated.value && sceneVisible.value && !document.hidden) sceneFollow.request()
}
function refreshScene() {
  if (animated.value && sceneVisible.value && !document.hidden) sceneFollow.request({ immediate: true })
  else sceneFollow.stop()
}
function scheduleSceneRefresh() {
  sceneFollow.stop()
  if (layoutRequest) cancelAnimationFrame(layoutRequest)
  layoutRequest = requestAnimationFrame(() => { layoutRequest = 0; refreshScene() })
}
function jumpToConversation(event?: Event, focusDeck = true) {
  if (!animated.value || !runway.value || !scene.value) return
  event?.preventDefault()
  const top = scrollY + runway.value.getBoundingClientRect().top + runway.value.offsetHeight - scene.value.offsetHeight
  window.scrollTo({ top, behavior: 'instant' as ScrollBehavior })
  if (location.hash !== '#moments') location.hash = 'moments'
  refreshScene()
  if (focusDeck) deck.value?.focus({ preventScroll: true })
}
function hashScene() {
  if (location.hash === '#moments') jumpToConversation(undefined, false)
  else refreshScene()
}
async function preparePixels() {
  if (sceneLoading || pixels || !pixelCanvas.value || !presenceFigure.value) return
  sceneLoading = true
  const attempt = ++initialization
  try {
    const renderer = await createMeetingPixels(pixelCanvas.value, presenceFigure.value, withBase('/media/first-meeting.png'))
    if (!mounted || attempt !== initialization) { renderer.dispose(); return }
    pixels = renderer
    pixelCanvas.value.dataset.ready = 'true'
    requestScene()
  } catch {
    if (mounted && attempt === initialization) {
      sceneFailed = true
      animated.value = false
      sceneFollow.stop()
      pixelCanvas.value?.setAttribute('data-ready', 'fallback')
      void syncPlayback()
    }
  } finally { if (attempt === initialization) sceneLoading = false }
}
function scenePreferences() {
  animated.value = !sceneFailed && !motion?.matches && !connection?.saveData && innerWidth > 800 && innerHeight > 620
  if (animated.value) void preparePixels()
  else { initialization++; sceneLoading = false; pixels?.dispose(); pixels = undefined }
  scheduleSceneRefresh()
}
function sceneVisibility() {
  pageHidden.value = document.hidden
  if (document.hidden) sceneFollow.stop()
  else refreshScene()
  void syncPlayback()
}
function ensureConversationFocus(event: FocusEvent) { if ((event.target as HTMLElement).matches(':focus-visible') && animated.value && sceneProgress.value < .97) jumpToConversation() }
function ensureIntroFocus() { if (animated.value && sceneProgress.value > .08 && sceneProgress.value < .95) jumpToConversation() }
const video = ref<HTMLVideoElement>()
const dragging = ref(false)
const playing = ref(false)
const hasSource = ref(false)
const wantsPlayback = ref(false)
const currentTheme = computed(() => themes.value[active.value])
let visible = false
let mounted = false
let playbackAttempt = 0
let observer: IntersectionObserver | undefined
let motion: MediaQueryList | undefined
let connection: (EventTarget & { saveData?: boolean }) | undefined
let pointerStart: { x: number; y: number; id: number } | undefined

function position(index: number) {
  return index === active.value ? 'active' : (index - active.value + 3) % 3 === 1 ? 'next' : 'previous'
}
function select(index: number) { if (animated.value && sceneProgress.value < .97) jumpToConversation(); active.value = (index + 3) % 3 }
function keyboard(event: KeyboardEvent) {
  if ((event.target as HTMLElement).closest('a,button')) return
  const actions: Record<string, () => void> = {
    ArrowLeft: () => select(active.value - 1), ArrowRight: () => select(active.value + 1),
    Home: () => select(0), End: () => select(2),
  }
  if (actions[event.key]) { event.preventDefault(); actions[event.key]() }
}
function pointerDown(event: PointerEvent) {
  if (!event.isPrimary || event.button !== 0 || (event.target as HTMLElement).closest('a,button')) return
  pointerStart = { x: event.clientX, y: event.clientY, id: event.pointerId }
  dragging.value = true
  deck.value?.setPointerCapture(event.pointerId)
}
function pointerUp(event: PointerEvent) {
  if (!pointerStart || pointerStart.id !== event.pointerId) return
  const x = event.clientX - pointerStart.x
  const y = event.clientY - pointerStart.y
  pointerStart = undefined
  dragging.value = false
  if (Math.abs(x) > 45 && Math.abs(x) > Math.abs(y) * 1.25) select(active.value + (x < 0 ? 1 : -1))
}
function cancelPointer() { pointerStart = undefined; dragging.value = false }
async function syncPlayback() {
  const attempt = ++playbackAttempt
  const element = video.value
  if (!mounted || !element) return
  if (!wantsPlayback.value || !visible || document.hidden || active.value !== 0 || (animated.value && sceneProgress.value < .7)) {
    element.pause()
    return
  }
  if (!hasSource.value) {
    hasSource.value = true
    element.src = withBase('/media/momo-moments.webm')
  }
  try { await element.play() } catch {
    // A pause/cut to another card invalidates older play promises, not the player’s intent.
    if (attempt === playbackAttempt && mounted && visible && !document.hidden && active.value === 0) wantsPlayback.value = false
  }
}
function toggleVideo() {
  const shouldPlay = !playing.value
  // An explicit play request brings the real surface fully into view. A sliver
  // of the controls can be clickable while the visibility gate still pauses it.
  if (shouldPlay && animated.value) jumpToConversation(undefined, false)
  wantsPlayback.value = shouldPlay
  void syncPlayback()
}
function clearSkipFocus(event: PointerEvent) {
  if (document.activeElement === skipLink.value && event.target !== skipLink.value) skipLink.value?.blur()
}
function respectPreferences() {
  if (motion?.matches || connection?.saveData) wantsPlayback.value = false
  scenePreferences()
  void syncPlayback()
}
watch(active, () => { void syncPlayback() })
onMounted(() => {
  mounted = true
  pageHidden.value = document.hidden
  motion = window.matchMedia('(prefers-reduced-motion: reduce)')
  connection = (navigator as Navigator & { connection?: typeof connection }).connection
  wantsPlayback.value = !motion.matches && !connection?.saveData
  observer = new IntersectionObserver(([entry]) => {
    visible = entry.isIntersecting
    void syncPlayback()
  }, { threshold: .2 })
  if (video.value) observer.observe(video.value)
  scenePreferences()
  // Initial hash navigation waits for Vue to apply the motion layout. Later resizes only repaint.
  hashRequest = requestAnimationFrame(() => { hashRequest = 0; hashScene() })
  sceneObserver = new IntersectionObserver(([entry]) => {
    sceneVisible.value = entry.isIntersecting
    if (entry.isIntersecting) refreshScene()
    else sceneFollow.stop()
  })
  if (runway.value) sceneObserver.observe(runway.value)
  resizeObserver = new ResizeObserver(() => { scenePreferences(); requestScene() })
  if (deck.value) resizeObserver.observe(deck.value)
  window.addEventListener('scroll', requestScene, { passive: true })
  window.addEventListener('resize', scenePreferences)
  window.addEventListener('hashchange', hashScene)
  document.addEventListener('visibilitychange', sceneVisibility)
  document.addEventListener('pointerdown', clearSkipFocus, true)
  motion.addEventListener('change', respectPreferences)
  connection?.addEventListener('change', respectPreferences)
})
onBeforeUnmount(() => {
  mounted = false
  initialization++
  pixels?.dispose()
  pixels = undefined
  video.value?.pause()
  observer?.disconnect()
  sceneObserver?.disconnect()
  resizeObserver?.disconnect()
  sceneFollow.dispose()
  cancelAnimationFrame(layoutRequest)
  cancelAnimationFrame(hashRequest)
  window.removeEventListener('scroll', requestScene)
  window.removeEventListener('resize', scenePreferences)
  window.removeEventListener('hashchange', hashScene)
  document.removeEventListener('visibilitychange', sceneVisibility)
  document.removeEventListener('pointerdown', clearSkipFocus, true)
  motion?.removeEventListener('change', respectPreferences)
  connection?.removeEventListener('change', respectPreferences)
})
</script>

<template>
  <a ref="skipLink" class="skip-link" href="#main">{{ t.skip }}</a>
  <SiteHeader :locale="locale" />
  <main id="main" :class="`locale-${locale}`">
    <div ref="runway" class="meeting-runway" :class="{ 'has-motion': animated }" data-testid="meeting-runway">
    <span id="moments" class="meeting-anchor" aria-hidden="true"></span>
    <div ref="scene" class="meeting-scene" :class="{ 'scene-resting': !sceneVisible || pageHidden }" data-testid="meeting-scene">
    <section class="first-meeting" aria-labelledby="intro-title">
      <div class="meeting-presence" aria-hidden="true">
        <img ref="presenceFigure" class="presence-figure" :src="withBase('/brand/momo-presence-v2.webp')" alt="" decoding="async">
      </div>
      <canvas ref="pixelCanvas" class="meeting-pixels" data-testid="meeting-pixels" aria-hidden="true"></canvas>
      <div class="intro" :aria-hidden="animated && sceneProgress > .16 ? true : undefined" :inert="animated && sceneProgress > .16" @focusin="ensureIntroFocus"><div class="intro-flourish" aria-hidden="true"><span></span>✧<span></span></div><h1 id="intro-title"><template v-for="(line, index) in t.hero" :key="line">{{ line }}<br v-if="index === 0"></template></h1><p><template v-for="(line, index) in t.intro" :key="line">{{ line }}<br v-if="index < 2"></template></p><a class="button primary" href="#start">{{ t.meet }}<span aria-hidden="true">✦</span></a><a class="scene-skip" href="#moments" data-testid="skip-meeting" @click="jumpToConversation">{{ sceneCopy.skip }}<span aria-hidden="true">↓</span></a><p class="intro-note"><template v-for="(line, index) in t.note" :key="line">{{ line }}<br v-if="index === 0"></template></p></div>
      <div class="conversation-chapter" :aria-hidden="!animated || sceneProgress < .75 ? true : undefined" :inert="!animated || sceneProgress < .75" @focusin="ensureConversationFocus"><h2>{{ sceneCopy.title }}</h2><p>{{ sceneCopy.body }}</p><a class="text-link" href="#start">{{ t.meet }}</a></div>
      <div class="keepsakes" @focusin="ensureConversationFocus">
        <div ref="deck" :class="{ 'is-dragging': dragging }" class="moment-deck" data-testid="moment-deck" role="region" :aria-roledescription="t.carouselRole" :aria-label="t.carousel" aria-describedby="deck-instructions" tabindex="0" @keydown="keyboard" @pointerdown="pointerDown" @pointerup="pointerUp" @pointercancel="cancelPointer" @lostpointercapture="cancelPointer">
          <article class="keepsake-card room-card" data-testid="moment-card" :data-position="position(0)" :aria-hidden="active !== 0" :inert="active !== 0" :aria-label="themes[0]"><div class="photo-mount"><video ref="video" data-testid="hero-video" :poster="withBase('/media/first-meeting.png')" preload="none" muted loop playsinline :aria-label="t.video" @play="playing = true" @pause="playing = false" @error="wantsPlayback = false; playing = false"></video><span class="photo-corner top-left" aria-hidden="true"></span><span class="photo-corner bottom-right" aria-hidden="true"></span><button class="video-toggle" data-testid="video-toggle" type="button" :aria-label="playing ? t.pause : t.play" @click="toggleVideo"><span aria-hidden="true">{{ playing ? 'Ⅱ' : '▷' }}</span>{{ playing ? t.paused : t.play }}</button></div><div class="photo-caption"><span>{{ t.caption }}</span><span class="caption-star" aria-hidden="true">✧</span></div></article>
          <article class="keepsake-card memory-card" data-testid="moment-card" :data-position="position(1)" :aria-hidden="active !== 1" :inert="active !== 1" :aria-label="themes[1]"><div class="binding" aria-hidden="true"></div><div class="paper-content"><span class="paper-motif" aria-hidden="true">✧</span><p class="handwritten-note">{{ t.memoryCard[0] }}<br>{{ t.memoryCard[1] }}</p><div class="memory-lines"><span v-for="label in t.memoryLabels" :key="label">{{ label }}</span></div><p class="paper-description">{{ t.memoryDescription[0] }}<br>{{ t.memoryDescription[1] }}</p><a class="text-link" :href="helpHref('data')">{{ t.memoryLink }}</a></div><span class="paper-stamp" aria-hidden="true">momo<br>✧</span></article>
          <article class="keepsake-card choices-card" data-testid="moment-card" :data-position="position(2)" :aria-hidden="active !== 2" :inert="active !== 2" :aria-label="themes[2]"><div class="choice-orbit" aria-hidden="true">✦</div><div class="paper-content"><span class="paper-motif" aria-hidden="true">✧</span><p class="handwritten-note">{{ t.choicesCard[0] }}<br>{{ t.choicesCard[1] }}</p><ul class="choice-list"><li v-for="choice in t.choices" :key="choice[0]"><span>{{ choice[0] }}</span>{{ choice[1] }}</li></ul><a class="text-link" :href="helpHref('start')">{{ t.choicesLink }}</a></div><span class="paper-stamp" aria-hidden="true">{{ t.stamp[0] }}<br>{{ t.stamp[1] }}</span></article>
        </div>
        <div class="deck-controls"><div class="deck-tabs" :aria-label="t.chooseMoment"><button v-for="(theme, index) in themes" :key="theme" data-testid="deck-tab" type="button" :aria-label="theme" :aria-pressed="active === index" @click="select(index)"><span></span></button></div><p class="deck-status" data-testid="deck-status" aria-live="polite" aria-atomic="true">{{ currentTheme }}</p><div class="deck-arrows"><button data-testid="deck-prev" type="button" :aria-label="t.previous" @click="select(active - 1)">←</button><button data-testid="deck-next" type="button" :aria-label="t.next" @click="select(active + 1)">→</button></div></div>
        <p id="deck-instructions" class="sr-only">{{ t.instructions }}</p><p class="keepsake-note"><span aria-hidden="true">✧</span>{{ t.keepsakeNote }}</p>
      </div>
    </section>
    <p class="meeting-scroll" aria-hidden="true"><span></span>{{ sceneCopy.scroll }}</p>
    </div>
    </div>
    <ProductChapters :locale="locale" />
    <section class="horizon-section" aria-labelledby="horizon-title"><h2 id="horizon-title">{{ t.horizon.title }}</h2><div class="horizon-columns"><article class="today"><h3><span aria-hidden="true">✦</span>{{ t.horizon.now }}</h3><ul><li v-for="item in t.horizon.nowItems" :key="item">{{ item }}</li></ul></article><article class="tomorrow"><h3>{{ t.horizon.vision }}</h3><p>{{ t.horizon.visionBody }}</p><p class="fine-note">{{ t.horizon.boundary }}</p></article></div></section>
    <section id="faq" class="faq-section product-section" aria-labelledby="faq-title"><div class="section-heading"><span class="section-flower" aria-hidden="true">✧</span><h2 id="faq-title">{{ t.faqTitle }}</h2></div><div class="faq-list"><details v-for="item in t.faqs" :key="item[0]"><summary>{{ item[0] }}<span aria-hidden="true">＋</span></summary><p>{{ item[1] }}</p></details></div></section>
    <section id="start" class="start" aria-labelledby="start-title"><span class="start-corner" aria-hidden="true">✧</span><div><h2 id="start-title">{{ t.startTitle[0] }}<br>{{ t.startTitle[1] }}</h2><p>{{ t.startBody[0] }}<br>{{ t.startBody[1] }}</p></div><div class="start-actions"><a class="button primary" :href="releaseUrl">{{ t.releases }}</a><a class="button secondary" :href="helpHref('start')">{{ t.guide }}</a><p class="release-note">{{ t.releaseNote[0] }}<br>{{ t.releaseNote[1] }}</p></div></section>
  </main>
  <footer class="site-footer"><div><a class="wordmark" :href="withBase(localePath(locale))" :aria-label="t.home"><img class="brand-mark" :src="withBase('/brand/momo-mark.svg')" alt=""><img class="brand-wordmark" :src="withBase('/brand/momo-wordmark.svg')" alt="momo"></a><p>{{ t.tagline }}</p></div><div class="footer-links"><a :href="helpHref('start')">{{ t.nav[3] }}</a><a href="https://github.com/Momoyeyu/momo">{{ t.source }}</a><button type="button" :aria-expanded="creditsOpen" aria-controls="credits" @click="creditsOpen = !creditsOpen">{{ t.credits }}</button></div><div v-if="creditsOpen" id="credits" class="credits"><p>{{ t.creditsBody }}</p><p><a :href="withBase('/media/hiyori-license.md')">{{ t.modelLicense }}</a> · <a :href="withBase('/media/hiyori-notice.txt')">{{ t.modelSource }}</a> · <a :href="withBase('/media/asset-notice.txt')">{{ t.assetSource }}</a> · <a :href="withBase('/brand/octicons-license.txt')">GitHub {{ t.license }}</a></p><p>{{ t.fonts }}</p><p><a :href="withBase('/fonts/notosanssc-OFL.txt')">Noto Sans SC {{ t.license }}</a> · <a :href="withBase('/fonts/notosansjp-OFL.txt')">Noto Sans JP {{ t.license }}</a> · <a :href="withBase('/fonts/nunito-OFL.txt')">Nunito {{ t.license }}</a></p></div><p class="copyright">© momo · {{ t.copyright }}</p></footer>
</template>
