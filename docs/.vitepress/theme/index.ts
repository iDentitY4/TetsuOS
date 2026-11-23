import { h } from 'vue'
import type { Theme } from 'vitepress'
import DefaultTheme from 'vitepress/theme'
import HomeScreens from './HomeScreens.vue'
import ImagePicker from './ImagePicker.vue'
import './custom.css'

export default {
  extends: DefaultTheme,
  Layout: () =>
    h(DefaultTheme.Layout, null, {
      'home-hero-after': () => h(ImagePicker),
      'home-features-after': () => h(HomeScreens),
    }),
} satisfies Theme
