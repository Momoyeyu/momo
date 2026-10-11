<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { useData, withBase } from 'vitepress'

const CORE_URL = 'https://cubism.live2d.com/sdk-web/core/05/live2dcubismcore.min.js'
const MODEL_URL = '/live2d/haru/Haru.model3.json'
const expressions = ['F01', 'F02', 'F03', 'F04', 'F05', 'F06', 'F07', 'F08']

const { isDark } = useData()
const host = ref<HTMLElement>()
const state = ref<'loading' | 'ready' | 'error' | 'paused'>('loading')
const error = ref('')
const bubble = ref('')
const continuousMotion = ref(true)
const inViewport = ref(true)
const hovered = ref(false)
const interactionActive = ref(false)
const pauseLabel = computed(() => continuousMotion.value ? '暂停动作' : '继续动作')
let app: import('pixi.js').Application | undefined
let model: import('pixi-live2d-display/cubism4').Live2DModel | undefined
let resizeObserver: ResizeObserver | undefined
let intersectionObserver: IntersectionObserver | undefined
let motionPreference: MediaQueryList | undefined
let expressionIndex = 0
let generation = 0
let interactionTimer: number | undefined
let originalBounds: { x: number; y: number; width: number; height: number } | undefined

function loadCore() {
  const global = window as typeof window & { Live2DCubismCore?: unknown; __momoCubismCorePromise?: Promise<void> }
  if (global.Live2DCubismCore) return Promise.resolve()
  if (global.__momoCubismCorePromise) return global.__momoCubismCorePromise
  global.__momoCubismCorePromise = new Promise<void>((resolve, reject) => {
    document.querySelector<HTMLScriptElement>(`script[src="${CORE_URL}"]`)?.remove()
    const script = document.createElement('script')
    let settled = false
    const timer = window.setTimeout(() => failed(new Error('Cubism Core 加载超时，请检查网络后重试。')), 15000)
    const finish = () => {
      if (settled) return false
      settled = true
      window.clearTimeout(timer)
      return true
    }
    const failed = (reason = new Error('Cubism Core 加载失败，请检查网络后重试。')) => {
      if (!finish()) return
      script.remove()
      global.__momoCubismCorePromise = undefined
      reject(reason)
    }
    script.addEventListener('load', () => {
      if (!global.Live2DCubismCore) {
        failed()
        return
      }
      if (finish()) resolve()
    }, { once: true })
    script.addEventListener('error', () => failed(), { once: true })
    script.src = CORE_URL
    script.crossOrigin = 'anonymous'
    document.head.appendChild(script)
  })
  return global.__momoCubismCorePromise
}

function assertWebGL() {
  const canvas = document.createElement('canvas')
  const context = canvas.getContext('webgl2') ?? canvas.getContext('webgl')
  if (!context) throw new Error('当前浏览器无法使用 WebGL。设计文档仍可正常阅读。')
  context.getExtension('WEBGL_lose_context')?.loseContext()
}

function fitModel() {
  if (!app || !model || !host.value || !originalBounds) return
  const width = Math.max(1, host.value.clientWidth)
  const height = Math.max(1, host.value.clientHeight)
  app.renderer.resize(width, height)
  const availableWidth = width * 0.88
  const availableHeight = height * 0.92
  const scale = Math.min(availableWidth / originalBounds.width, availableHeight / originalBounds.height)
  model.scale.set(scale)
  model.x = (width - originalBounds.width * scale) / 2 - originalBounds.x * scale
  model.y = (height - originalBounds.height * scale) / 2 - originalBounds.y * scale
  app.renderer.render(app.stage)
}

function updateTicker() {
  if (!app) return
  const shouldRun = !document.hidden && inViewport.value && (continuousMotion.value || interactionActive.value)
  if (shouldRun) {
    app.ticker.start()
    state.value = 'ready'
  } else {
    app.ticker.stop()
    app.renderer.render(app.stage)
    if (model) state.value = 'paused'
  }
}

function cancelInteraction() {
  if (interactionTimer) window.clearTimeout(interactionTimer)
  interactionTimer = undefined
  interactionActive.value = false
}

function allowInteraction(duration = 4200) {
  interactionActive.value = true
  if (interactionTimer) window.clearTimeout(interactionTimer)
  interactionTimer = window.setTimeout(() => {
    interactionTimer = undefined
    interactionActive.value = false
    updateTicker()
  }, duration)
  updateTicker()
}

function canvasPoint(event: PointerEvent) {
  if (!app) return { x: 0, y: 0 }
  const canvas = app.view as HTMLCanvasElement
  const rect = canvas.getBoundingClientRect()
  return {
    x: ((event.clientX - rect.left) / rect.width) * app.renderer.screen.width,
    y: ((event.clientY - rect.top) / rect.height) * app.renderer.screen.height,
  }
}

function onPointerMove(event: PointerEvent) {
  if (!model || !hovered.value) return
  const point = canvasPoint(event)
  model.focus(point.x, point.y)
}

async function greet() {
  if (!model) return
  allowInteraction()
  await Promise.allSettled([model.motion('TapBody', 0), model.expression('F01')])
  bubble.value = '你好呀。今天想一起做点什么？'
}

async function nextExpression() {
  if (!model) return
  allowInteraction(2600)
  expressionIndex = (expressionIndex + 1) % expressions.length
  try {
    const changed = await model.expression(expressions[expressionIndex])
    bubble.value = changed ? `表情 ${expressions[expressionIndex]} · Haru 官方 sample` : '这个表情暂时没有切换，再试一次吧。'
  } catch {
    bubble.value = '表情切换遇到问题了，再试一次吧。'
  }
}

function togglePause() {
  continuousMotion.value = !continuousMotion.value
  if (!continuousMotion.value) cancelInteraction()
  updateTicker()
}

function onVisibility() {
  updateTicker()
}

function onMotionPreference(event: MediaQueryListEvent) {
  if (event.matches) {
    continuousMotion.value = false
    cancelInteraction()
  }
  updateTicker()
}

function disposeRuntime() {
  cancelInteraction()
  resizeObserver?.disconnect()
  resizeObserver = undefined
  intersectionObserver?.disconnect()
  intersectionObserver = undefined
  document.removeEventListener('visibilitychange', onVisibility)
  motionPreference?.removeEventListener('change', onMotionPreference)
  motionPreference = undefined
  const currentModel = model
  model = undefined
  if (currentModel && !currentModel.destroyed) {
    app?.stage.removeChild(currentModel)
    currentModel.destroy({ children: true, texture: true, baseTexture: true })
  }
  if (app) {
    app.ticker.stop()
    app.destroy(true, { children: true, texture: true, baseTexture: true })
    app = undefined
  }
  originalBounds = undefined
  host.value?.replaceChildren()
}

function cleanup() {
  generation += 1
  disposeRuntime()
}

async function initialize() {
  generation += 1
  const request = generation
  disposeRuntime()
  state.value = 'loading'
  error.value = ''
  bubble.value = ''
  inViewport.value = true
  await nextTick()
  try {
    if (!host.value) return
    assertWebGL()
    await loadCore()
    if (request !== generation || !host.value) return
    const PIXI = await import('pixi.js')
    if (request !== generation || !host.value) return
    ;(window as typeof window & { PIXI?: typeof PIXI }).PIXI = PIXI
    const live2d = await import('pixi-live2d-display/cubism4')
    if (request !== generation || !host.value) return
    live2d.config.sound = false
    live2d.config.logLevel = live2d.config.LOG_LEVEL_ERROR
    app = new PIXI.Application({
      backgroundAlpha: 0,
      antialias: true,
      autoDensity: true,
      resolution: Math.min(window.devicePixelRatio || 1, 2),
      width: Math.max(1, host.value.clientWidth),
      height: Math.max(1, host.value.clientHeight),
    })
    app.ticker.maxFPS = 30
    app.ticker.stop()
    if (request !== generation || !host.value) {
      disposeRuntime()
      return
    }
    const canvas = app.view as HTMLCanvasElement
    canvas.setAttribute('aria-label', 'Haru Live2D 互动模型')
    canvas.tabIndex = -1
    host.value.replaceChildren(canvas)
    const loadedModel = await live2d.Live2DModel.from(withBase(MODEL_URL), { autoInteract: false, ticker: app.ticker })
    if (request !== generation || !app || !host.value) {
      loadedModel.destroy({ children: true, texture: true, baseTexture: true })
      disposeRuntime()
      return
    }
    model = loadedModel
    const bounds = model.getLocalBounds()
    originalBounds = { x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height }
    app.stage.addChild(model)
    fitModel()
    resizeObserver = new ResizeObserver(fitModel)
    resizeObserver.observe(host.value)
    intersectionObserver = new IntersectionObserver(([entry]) => {
      inViewport.value = entry.isIntersecting
      updateTicker()
    }, { threshold: 0.08 })
    intersectionObserver.observe(host.value)
    canvas.addEventListener('pointerenter', () => {
      hovered.value = true
    })
    canvas.addEventListener('pointerleave', () => {
      hovered.value = false
    })
    canvas.addEventListener('pointermove', onPointerMove)
    canvas.addEventListener('pointerup', (event) => {
      if (!model) return
      const point = canvasPoint(event)
      model.tap(point.x, point.y)
      allowInteraction()
    })
    document.addEventListener('visibilitychange', onVisibility)
    motionPreference = window.matchMedia('(prefers-reduced-motion: reduce)')
    continuousMotion.value = !motionPreference.matches
    motionPreference.addEventListener('change', onMotionPreference)
    updateTicker()
  } catch (reason) {
    if (request !== generation) return
    disposeRuntime()
    if (request !== generation) return
    error.value = reason instanceof Error ? reason.message : 'Live2D 模型加载失败。'
    state.value = 'error'
  }
}

onMounted(initialize)
onBeforeUnmount(cleanup)
</script>

<template>
  <section class="live2d-stage" :class="{ 'is-dark': isDark }" :data-live2d-state="state" aria-label="Live2D 互动预览">
    <div class="scene-orbit" aria-hidden="true"></div>
    <div class="scene-ground" aria-hidden="true"></div>
    <p class="scene-note">与你同频</p>
    <div ref="host" class="live2d-canvas-host"></div>
    <div v-if="state === 'loading'" class="live2d-loading" role="status">正在邀请 Haru 来到桌面…</div>
    <div v-if="state === 'error'" class="live2d-error" role="alert"><strong>互动预览暂时没有加载成功</strong><span>{{ error }}</span><div><button type="button" @click="initialize">重试</button><a :href="withBase('/guide/credits')">查看素材与兼容说明</a></div></div>
    <p v-else-if="bubble" class="live2d-bubble" aria-live="polite">{{ bubble }}</p>
    <div class="live2d-actions" aria-label="Live2D 互动控制">
      <button type="button" :disabled="state === 'loading' || state === 'error'" @click="greet">打个招呼</button>
      <button type="button" :disabled="state === 'loading' || state === 'error'" @click="nextExpression">换个表情</button>
      <button type="button" :disabled="state === 'loading' || state === 'error'" @click="togglePause">{{ pauseLabel }}</button>
    </div>
    <p class="live2d-caption">Live2D 互动预览 · 不连接对话服务</p>
  </section>
</template>
