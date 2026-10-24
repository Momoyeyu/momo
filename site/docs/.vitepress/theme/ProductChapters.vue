<script setup lang="ts">
import { withBase } from 'vitepress'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { copy } from './product-copy.mjs'
import { helpLinks } from './product-content.mjs'
import { clamp } from './meeting-motion.mjs'
import { archiveGlyph, phase } from './chapter-motion.mjs'
import { createScrollFollow } from './scroll-follow.mjs'
import { sceneProgress } from './scroll-beats.mjs'
import './chapters.css'
import EchoScene from './EchoScene.vue'

const props = defineProps<{ locale: 'zh' | 'en' | 'ja' }>()
const t = computed(() => copy[props.locale])
const labels = computed(() => ({
  zh: { index: '在这里，慢慢展开', sections: ['熟悉的她', '记忆与存档', '字句与回应', '你的相处方式'], imprint: '留下经历，仍是她自己。', memory: ['记忆', '存档', '备份'], hello: '你好，momo。', reply: '你回来啦！', score: '一次回应的节奏', tracks: ['字句', '神情', '声音'], constellation: '由你选择，慢慢拓展。' },
  en: { index: 'A little more to discover', sections: ['A familiar presence', 'Memories & saves', 'Words & responses', 'Your own way'], imprint: 'More shared days. Still herself.', memory: ['Memories', 'Saves', 'Backups'], hello: 'Hello, momo.', reply: 'You’re back!', score: 'The rhythm of a reply', tracks: ['Words', 'Expression', 'Voice'], constellation: 'More possibilities, chosen by you.' },
  ja: { index: 'ここから、少しずつ', sections: ['いつもの彼女', '記憶とセーブ', '言葉と返事', 'あなたらしく'], imprint: '思い出が増えても、彼女らしく。', memory: ['記憶', 'セーブ', 'バックアップ'], hello: 'こんにちは、momo。', reply: 'おかえり！', score: 'ひとつの返事のリズム', tracks: ['言葉', '表情', '声'], constellation: 'あなたが選ぶ、新しい可能性。' },
}[props.locale]))
const ids = ['together', 'memory', 'voice', 'extensions']
const beats = ['soul', 'memory', 'echo', 'paths']
const container = ref<HTMLElement>()
const enhanced = ref(false)
const echoEnhanced = ref(false)
const echoAvailable = ref(true)
const progress = ref([1, 1, 1, 1])
const helpHref = (key: keyof typeof helpLinks) => withBase(helpLinks[key]) + (props.locale === 'zh' ? '' : `?lang=${props.locale}`)
const glyphs = computed(() => Array.from(t.value.memoryLabels.join('　')).map((char, index, all) => ({ char, ...archiveGlyph(index, all.length, progress.value[1]) })))
const soul = computed(() => phase(progress.value[0], .05, .82))
const stitch = computed(() => phase(progress.value[3], .08, .83))
let media: MediaQueryList | undefined
let connection: (EventTarget & { saveData?: boolean }) | undefined
let observer: IntersectionObserver | undefined
let resize: ResizeObserver | undefined
let clocks: ReturnType<typeof createScrollFollow>[] = []
let sections: HTMLElement[] = []
const visible = new Set<number>()

function measure(index: number) {
  const section = sections[index]
  const stage = section?.querySelector<HTMLElement>('.chapter-stage')
  if (!section || !stage || !(index === 2 ? echoEnhanced.value : enhanced.value)) return 1
  return clamp(-section.getBoundingClientRect().top / Math.max(1, section.offsetHeight - stage.offsetHeight))
}
function write(index: number, value: number) {
  const motion = (index === 2 ? echoEnhanced.value : enhanced.value) ? sceneProgress(value, beats[index]) : 1
  progress.value[index] = motion
  const section = sections[index]
  if (section) {
    section.dataset.progress = value.toFixed(4)
    section.dataset.animationProgress = motion.toFixed(4)
  }
}
function request() {
  if ((!enhanced.value && !echoEnhanced.value) || document.hidden) return
  for (const index of visible) clocks[index]?.request()
}
function preferences() {
  echoEnhanced.value = echoAvailable.value && !media?.matches && !connection?.saveData && innerHeight > 600
  enhanced.value = !media?.matches && !connection?.saveData && innerWidth > 1000 && innerHeight > 740
  for (let index = 0; index < clocks.length; index++) {
    if ((index === 2 ? echoEnhanced.value : enhanced.value) && !document.hidden) clocks[index].request({ immediate: true })
    else { clocks[index].stop(); write(index, 1) }
  }
}
function visibility() {
  if (document.hidden) clocks.forEach(clock => clock.stop())
  else for (const index of visible) clocks[index]?.request({ immediate: true })
}
function directNavigation() { clocks.forEach(clock => clock.request({ immediate: true })) }
onMounted(() => {
  sections = Array.from(container.value?.querySelectorAll<HTMLElement>('.story-chapter') || [])
  clocks = sections.map((_, index) => createScrollFollow({ read: () => measure(index), write: (value: number) => write(index, value) }))
  media = matchMedia('(prefers-reduced-motion: reduce)')
  connection = (navigator as Navigator & { connection?: typeof connection }).connection
  preferences()
  observer = new IntersectionObserver(entries => {
    for (const entry of entries) {
      const index = sections.indexOf(entry.target as HTMLElement)
      if (entry.isIntersecting) { visible.add(index); if ((index === 2 ? echoEnhanced.value : enhanced.value) && !document.hidden) clocks[index]?.request({ immediate: true }) }
      else { visible.delete(index); clocks[index]?.stop() }
    }
  })
  sections.forEach(section => observer?.observe(section))
  resize = new ResizeObserver(preferences)
  if (container.value) resize.observe(container.value)
  window.addEventListener('scroll', request, { passive: true })
  window.addEventListener('resize', preferences)
  window.addEventListener('hashchange', directNavigation)
  document.addEventListener('visibilitychange', visibility)
  media.addEventListener('change', preferences)
  connection?.addEventListener('change', preferences)
})
onBeforeUnmount(() => {
  clocks.forEach(clock => clock.dispose())
  observer?.disconnect(); resize?.disconnect()
  window.removeEventListener('scroll', request)
  window.removeEventListener('resize', preferences)
  window.removeEventListener('hashchange', directNavigation)
  document.removeEventListener('visibilitychange', visibility)
  media?.removeEventListener('change', preferences)
  connection?.removeEventListener('change', preferences)
})
</script>

<template>
  <div ref="container" class="product-chapters" :class="{ 'chapters-enhanced': enhanced, 'echo-enhanced': echoEnhanced }">
    <nav class="chapter-index" :aria-label="labels.index" data-testid="chapter-index"><span>{{ labels.index }}</span><div><a v-for="(label, index) in labels.sections" :key="label" :href="`#${ids[index]}`">{{ label }}<span aria-hidden="true">✧</span></a></div></nav>

    <section id="together" class="story-chapter soul-chapter" data-testid="soul-chapter" aria-labelledby="relationship-title" :style="{ '--soul-lock': soul, '--soul-drift': 1 - soul }">
      <div class="chapter-stage">
        <div class="chapter-copy"><h2 id="relationship-title">{{ t.relationship.title[0] }}<br>{{ t.relationship.title[1] }}</h2><p class="chapter-lead">{{ t.relationship.lead }}</p><p>{{ t.relationship.body }}</p><p class="chapter-aside">{{ t.relationship.closing }}</p></div>
        <div class="soul-impression" aria-hidden="true">
          <div class="ink-register"><img v-for="n in 3" :key="n" :class="`ink-plate plate-${n}`" :src="withBase('/brand/momo-presence-v2.webp')" alt="" loading="lazy"><span class="registration top"></span><span class="registration bottom"></span></div>
          <svg class="soul-threads" viewBox="0 0 640 650" fill="none"><path d="M55 192C-16 415 263 606 464 506S707 188 462 100"/><path d="M134 523C403 674 663 351 554 157S168 12 84 304"/><path class="thread-gold" pathLength="1" :stroke-dashoffset="1-soul" d="M40 420C110 114 463 33 553 223S410 609 132 484"/><g v-for="n in 28" :key="n" :transform="`translate(${72+(n*73)%496} ${76+(n*137)%489})`" :opacity=".12+Math.sin(n)*.07"><path d="M-2 0h4M0-2v4"/></g></svg>
          <div class="soul-signature"><img class="signature-echo echo-a" :src="withBase('/brand/momo-wordmark.svg')" alt=""><img class="signature-echo echo-b" :src="withBase('/brand/momo-wordmark.svg')" alt=""><img class="signature-fixed" :src="withBase('/brand/momo-wordmark.svg')" alt=""><p>{{ labels.imprint }}</p></div>
          <div class="soul-witnesses"><span v-for="(line,index) in t.relationship.paper" :key="line" :class="`witness-${index}`"><i></i>{{ line }}</span></div>
        </div>
        <p class="chapter-footnote soul-footnote">{{ t.relationship.aside }}</p>
      </div>
    </section>

    <section id="memory" class="story-chapter archive-chapter" data-testid="archive-chapter" aria-labelledby="memory-title" :style="{ '--archive-open': phase(progress[1], .05, .82) }">
      <div class="chapter-stage">
        <div class="chapter-copy"><h2 id="memory-title">{{ t.memory.title[0] }}<br>{{ t.memory.title[1] }}</h2><p class="chapter-lead">{{ t.memory.lead }}</p><a class="text-link" :href="helpHref('data')">{{ t.memory.link }}</a><div class="archive-source" aria-hidden="true"><span v-for="label in t.memoryLabels" :key="label">{{ label }}</span><svg viewBox="0 0 320 160" fill="none"><path d="M2 35C230 190 298-43 310 45M2 84C180-15 259 215 310 98M2 125C127 178 212 39 310 130"/></svg></div></div>
        <div class="archive-papers"><div class="archive-shadow shadow-a" aria-hidden="true"></div><div class="archive-shadow shadow-b" aria-hidden="true"></div><div class="archive-binding" aria-hidden="true"></div><article v-for="(item,index) in t.memory.items" :key="item[0]" class="archive-entry"><span class="entry-tab">{{ labels.memory[index] }}</span><h3>{{ item[0] }}</h3><p>{{ item[1] }}</p><span class="entry-seal" aria-hidden="true">{{ ['◇','✧','⌑'][index] }}</span></article></div>
        <div class="archive-fragments" aria-hidden="true"><span v-for="(glyph,index) in glyphs" :key="index" :style="{left:`${glyph.x}%`,top:`${glyph.y}%`,opacity:glyph.opacity,transform:`rotate(${glyph.rotation}deg)`}">{{ glyph.char }}</span></div>
        <p class="chapter-footnote archive-footnote">{{ t.memory.note }}</p>
      </div>
    </section>

    <section id="voice" class="story-chapter response-chapter" data-testid="response-chapter" aria-labelledby="voice-title" :style="{ '--response-scan': phase(progress[2], .02, .9) }">
      <div class="chapter-stage">
        <div class="chapter-copy"><h2 id="voice-title">{{ t.voice.title[0] }}<br>{{ t.voice.title[1] }}</h2><p class="chapter-lead">{{ t.voice.heading }}</p><p>{{ t.voice.body }}</p><p class="chapter-aside">{{ t.voice.detail }}</p><a class="text-link" :href="helpHref('voice')">{{ t.voice.link }}</a></div>
        <EchoScene @unavailable="echoAvailable = false; preferences()" :progress="progress[2]" :enhanced="echoEnhanced" />
        <p class="chapter-footnote response-footnote">{{ t.voice.note }}</p>
      </div>
    </section>

    <section id="extensions" class="story-chapter paths-chapter" data-testid="paths-chapter" aria-labelledby="extensions-title" :style="{ '--stitch': stitch }">
      <div class="chapter-stage"><div class="chapter-copy"><h2 id="extensions-title">{{ t.extensions.title[0] }}<br>{{ t.extensions.title[1] }}</h2><p class="chapter-lead">{{ t.extensions.body }}</p><p>{{ t.extensions.detail }}</p><a class="text-link" :href="helpHref('start')">{{ t.extensions.link }}</a></div><div class="possibility-map" aria-hidden="true"><svg viewBox="0 0 640 420" fill="none"><g class="map-grid"><path v-for="n in 9" :key="`v${n}`" :d="`M${n*64} 0V420`"/><path v-for="n in 6" :key="`h${n}`" :d="`M0 ${n*64}H640`"/></g><path class="map-branch" pathLength="1" :stroke-dashoffset="1-stitch" d="M95 210H210Q230 210 245 190L340 80Q355 63 380 63H520M95 210H540M95 210H220Q239 210 255 233L330 338Q347 360 370 360H520"/><g v-for="(y,index) in [63,210,360]" :key="y" :transform="`translate(${[520,540,520][index]} ${y})`"><path class="map-knot" d="M0-7L7 0 0 7-7 0Z"/></g><circle class="map-origin" cx="95" cy="210" r="29"/><path class="map-spark" d="M95 197L99 206 108 210 99 214 95 223 91 214 82 210 91 206Z"/></svg><span v-for="(label,index) in t.extensions.labels" :key="label" :class="`map-label map-label-${index}`">{{ label }}</span><p>{{ labels.constellation }}</p></div></div>
    </section>
  </div>
</template>
