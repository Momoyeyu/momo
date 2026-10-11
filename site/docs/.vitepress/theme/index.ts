import type { Theme } from 'vitepress'
import DefaultTheme from 'vitepress/theme'
import Live2DStage from './Live2DStage.vue'
import MermaidDiagram from './MermaidDiagram.vue'
import MomoHome from './MomoHome.vue'
import './editorial.css'

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    app.component('Live2DStage', Live2DStage)
    app.component('MermaidDiagram', MermaidDiagram)
    app.component('MomoHome', MomoHome)
  },
} satisfies Theme
