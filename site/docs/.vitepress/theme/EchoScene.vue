<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { phase } from './chapter-motion.mjs'
import { sampleEcho } from './echo-motion.mjs'
const props = defineProps<{ progress: number, enhanced: boolean }>()
const emit = defineEmits<{ unavailable: [] }>()
const canvas = ref<HTMLCanvasElement>()
const available = ref(false)
let context: CanvasRenderingContext2D | null = null
let observer: ResizeObserver | undefined
let width = 0, height = 0
function draw() {
  if (!context || !canvas.value || document.hidden) return
  const frame = sampleEcho(props.enhanced ? props.progress : 1, width, height)
  context.clearRect(0,0,width,height)
  context.font = '8px ui-monospace, monospace'
  context.textAlign = 'center'
  // A barely visible continuous fibre gives the glyphs a material to inhabit.
  let previous: typeof frame.glyphs[number] | undefined
  for (const glyph of frame.glyphs) {
    if (previous && !glyph.start) {
      context.globalAlpha = Math.min(glyph.opacity, previous.opacity)*.28
      context.strokeStyle = glyph.gold ? '#e7be77' : '#f3ecdf'
      context.lineWidth = .65
      context.beginPath(); context.moveTo(previous.x,previous.y); context.lineTo(glyph.x,glyph.y); context.stroke()
    }
    previous = glyph
    context.globalAlpha = glyph.opacity
    context.fillStyle = glyph.gold ? '#e7be77' : '#f3ecdf'
    context.font = `${glyph.size}px ui-monospace, monospace`
    context.fillText(glyph.char, glyph.x, glyph.y)
  }
  context.globalAlpha = 1
  canvas.value.dataset.frame = (props.enhanced ? props.progress : 1).toFixed(4)
}
function measure() {
  if (!canvas.value || !context) return
  const box = canvas.value.getBoundingClientRect()
  width = box.width; height = box.height
  const dpr = Math.min(devicePixelRatio || 1, 1.5)
  canvas.value.width = Math.round(width*dpr); canvas.value.height = Math.round(height*dpr)
  context.setTransform(dpr,0,0,dpr,0,0)
  draw()
}
onMounted(() => {
  try { context = canvas.value?.getContext('2d') ?? null } catch { context = null }
  available.value = !!context
  if (!context) { emit('unavailable'); return }
  observer = new ResizeObserver(measure)
  observer.observe(canvas.value!)
  document.addEventListener('visibilitychange', draw)
  measure()
})
watch(() => [props.progress, props.enhanced], draw)
onBeforeUnmount(() => { observer?.disconnect(); document.removeEventListener('visibilitychange', draw); context = null })
</script>
<template>
  <div class="echo-scene" :class="{'echo-static': !enhanced || !available}" :style="{'--echo-question': enhanced && available ? phase(progress,.70,.8) : 1}">
    <canvas ref="canvas" class="echo-canvas" aria-hidden="true"></canvas>
    <p class="echo-question">Can you feel my heart<span>?</span></p>
  </div>
</template>
