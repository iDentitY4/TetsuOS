<script setup lang="ts">
import { withBase } from 'vitepress'

// Shown under the home page's feature cards. Markdown's raw HTML is off for the whole site
// (the feature notes use <angle> placeholders in prose), so this lives in the theme.
const screens = [
  {
    src: '/screenshots/home.png',
    alt: 'The home screen: a grid of tiles for RECENT, FAVORITES, each console with games, SCREENSHOTS and SETTINGS',
  },
  {
    src: '/screenshots/pause.png',
    alt: 'The in-game menu over a game, with save and load state, display, options, achievements and switch game',
  },
  { src: '/screenshots/switcher.png', alt: 'The game switcher showing recent games as covers' },
]
</script>

<template>
  <div class="home-screens">
    <img v-for="screen in screens" :key="screen.src" :src="withBase(screen.src)" :alt="screen.alt" />
  </div>
</template>
