import DefaultTheme from 'vitepress/theme'
import DiagramFrame from './DiagramFrame.vue'
import Floaties from './Floaties.vue'
import SvgDiagram from './SvgDiagram.vue'
import './custom.css'

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    app.component('DiagramFrame', DiagramFrame)
    app.component('Floaties', Floaties)
    app.component('SvgDiagram', SvgDiagram)
  }
}
