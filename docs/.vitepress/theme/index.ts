import DefaultTheme from 'vitepress/theme'
import DiagramFrame from './DiagramFrame.vue'
import './custom.css'

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    app.component('DiagramFrame', DiagramFrame)
  }
}
