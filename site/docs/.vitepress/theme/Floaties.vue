<script setup>
const props = defineProps({
  count: { type: Number, default: 14 },
})
const glyphs = ['✦', '♡', '✧', '☁', '♪', '⋆', '❀', '˚', '☾', '♡']
const hues = ['#f7a8c4', '#a78bfa', '#8fd8c9', '#f9c7a0', '#a5c8ff']

function rnd(seed, min, max) {
  const x = Math.sin(seed * 9973.1) * 43758.5453
  return min + (x - Math.floor(x)) * (max - min)
}

const items = Array.from({ length: props.count }, (_, i) => ({
  glyph: glyphs[i % glyphs.length],
  left: rnd(i + 1, 2, 96),
  top: rnd(i + 11, 4, 92),
  size: rnd(i + 21, 12, 26),
  dur: rnd(i + 31, 9, 20),
  delay: -rnd(i + 41, 0, 20),
  drift: rnd(i + 51, -30, 30),
  color: hues[i % hues.length],
  spin: rnd(i + 61, 0, 360),
}))
</script>

<template>
  <div class="floaties" aria-hidden="true">
    <span
      v-for="(it, i) in items"
      :key="i"
      class="floaty"
      :style="{
        left: it.left + '%',
        top: it.top + '%',
        fontSize: it.size + 'px',
        color: it.color,
        animationDuration: it.dur + 's',
        animationDelay: it.delay + 's',
        '--drift': it.drift + 'px',
        '--spin': it.spin + 'deg',
      }"
    >{{ it.glyph }}</span>
  </div>
</template>
