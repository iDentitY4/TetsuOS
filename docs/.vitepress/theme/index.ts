import { h } from 'vue'
import type { Theme } from 'vitepress'
import DefaultTheme from 'vitepress/theme'
import HeroScreen from './HeroScreen.vue'
import HomeScreens from './HomeScreens.vue'
import ImagePicker from './ImagePicker.vue'
import '@fontsource/silkscreen/latin-400.css'
import './custom.css'

export default {
  extends: DefaultTheme,
  Layout: () =>
    h(DefaultTheme.Layout, null, {
      'home-hero-image': () => h(HeroScreen),
      'home-hero-after': () => h(ImagePicker),
      'home-features-after': () => h(HomeScreens),
    }),
} satisfies Theme
