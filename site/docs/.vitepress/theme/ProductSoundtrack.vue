<script setup lang="ts">
import { withBase } from 'vitepress'
import { computed, ref } from 'vue'
import { soundtrackTracks } from '../../../scripts/soundtrack-tracks.mjs'
import { copy } from './product-copy.mjs'

const props = defineProps<{ locale: 'zh' | 'en' | 'ja' }>()
const words = computed(() => copy[props.locale].soundtrack)
const trackNames = { zh: 1, en: 2, ja: 3 } as const
const selected = ref(0)
const player = ref<HTMLAudioElement>()
const playing = ref(false)
const failed = ref(false)
const source = (index: number) => withBase(`/soundtrack/${soundtrackTracks[index][0]}-ost.mp3`)
const title = (index: number) => soundtrackTracks[index][trackNames[props.locale]]

async function select(index: number) {
  if (!player.value) return
  failed.value = false
  if (selected.value !== index) {
    selected.value = index
    player.value.setAttribute('src', source(index))
  }
  try {
    await player.value.play()
  } catch (error) {
    if (!(error instanceof DOMException && error.name === 'AbortError')) failed.value = true
  }
}
</script>

<template>
  <section id="soundtrack" class="soundtrack product-section" aria-labelledby="soundtrack-title">
    <div>
      <p class="soundtrack-label">{{ words.label }}</p>
      <h2 id="soundtrack-title">Phase &amp; Presence</h2>
      <p class="soundtrack-description">{{ words.description }}</p>
      <div class="soundtrack-score" :class="{ 'is-playing': playing }" aria-hidden="true">
        <span v-for="note in 5" :key="note"></span>
      </div>
    </div>
    <div class="soundtrack-player">
      <ol :aria-label="words.label">
        <li v-for="(track, index) of soundtrackTracks" :key="track[0]">
          <button type="button" :aria-current="selected === index ? 'true' : undefined" @click="select(index)">
            <span class="track-action" aria-hidden="true">{{ selected === index && playing ? '♪' : '▷' }}</span>
            <span>{{ title(index) }}</span>
            <span class="track-duration">{{ track[4] }}</span>
          </button>
        </li>
      </ol>
      <audio ref="player" :src="source(0)" :aria-label="title(selected)" controls preload="none"
        @play="playing = true" @pause="playing = false" @ended="playing = false" @error="failed = true; playing = false" />
      <p v-if="failed" role="status">{{ words.error }}</p>
    </div>
  </section>
</template>

<style scoped>
.soundtrack {
  display: grid;
  grid-template-columns: minmax(0, .9fr) minmax(0, 1.1fr);
  align-items: center;
  gap: clamp(1.75rem, 5vw, 5rem);
  padding-block: clamp(2.75rem, 7vw, 6rem);
  color: #f7ebd9;
}
.soundtrack-label {
  color: #e7be77;
  margin: 0 0 .65rem;
}
.soundtrack h2 {
  margin: 0;
  font-size: clamp(2rem, 5vw, 4rem);
  line-height: 1.06;
  text-wrap: balance;
}
.soundtrack-description { margin-top: 1rem; max-width: 30rem; color: #d5c5cf; }
.soundtrack-score { display: flex; gap: .5rem; align-items: center; height: 2.5rem; margin-top: 1.5rem; }
.soundtrack-score span {
  width: 3px;
  height: 75%;
  background: #e7be77;
  border-radius: 2px;
  transform: scaleY(.52);
}
.soundtrack-score span:nth-child(2n) { transform: scaleY(1); }
.soundtrack-score.is-playing span { animation: soundtrack-pulse 1.1s ease-in-out infinite alternate; }
.soundtrack-score.is-playing span:nth-child(2n) { animation-delay: -.6s; }
@keyframes soundtrack-pulse { to { transform: scaleY(.3); } }
.soundtrack-player { min-width: 0; border-top: 1px solid #54474e; }
.soundtrack-player ol { list-style: none; margin: 0; padding: 0; }
.soundtrack-player button {
  display: grid;
  grid-template-columns: 1.2rem minmax(0, 1fr) auto;
  align-items: center;
  column-gap: .7rem;
  appearance: none;
  width: 100%;
  padding: .65rem .75rem;
  border: 0;
  background: transparent;
  font: inherit;
  cursor: pointer;
  border-bottom: 1px solid #54474e;
  text-align: start;
  color: inherit;
}
.soundtrack-player button:hover { background: #e7be770d; }
.soundtrack-player button[aria-current] {
  background: #e7be7714;
  box-shadow: inset 2px 0 #e7be77;
}
.soundtrack-player button:focus-visible { outline: 2px solid #e7be77; outline-offset: -2px; }
.track-action { color: #e7be77; }
.track-duration { color: #d5c5cf; font-variant-numeric: tabular-nums; }
.soundtrack-player audio { width: 100%; height: 2.5rem; margin-top: .9rem; color-scheme: dark; }
@media (max-width: 720px) {
  .soundtrack { grid-template-columns: minmax(0, 1fr); }
  .soundtrack-score { display: none; }
}
@media (prefers-reduced-motion: reduce) {
  .soundtrack-score.is-playing span { animation: none; }
}
</style>
