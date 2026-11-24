<script setup lang="ts">
import { withBase } from 'vitepress'

// Shown under the home page's feature cards. Markdown's raw HTML is off for the whole site
// (the feature notes use <angle> placeholders in prose), so this lives in the theme.
const screens = [
  {
    src: '/screenshots/pause.png',
    alt: 'The in-game menu over a game, with save and load state, display, options, achievements and switch game',
  },
  { src: '/screenshots/switcher.png', alt: 'The game switcher showing recent games as covers' },
  {
    src: '/screenshots/achievements.png',
    alt: "A game's RetroAchievements: those almost there, locked and unlocked, with points",
  },
]
</script>

<template>
  <p class="home-hint" aria-hidden="true">Click opens - / search</p>
  <div class="home-screens">
    <img v-for="screen in screens" :key="screen.src" :src="withBase(screen.src)" :alt="screen.alt" />
  </div>
</template>
