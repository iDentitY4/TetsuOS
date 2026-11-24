<script setup lang="ts">
import { withBase } from 'vitepress'

// The home page's hero shows the launcher itself rather than the logo, which is in the
// screen's corner and the nav bar anyway
</script>

<template>
  <img
    class="hero-screen"
    :src="withBase('/screenshots/home.png')"
    width="640"
    height="480"
    alt="Tetsu OS's home screen: a grid of tiles for RECENT, FAVORITES, each console with games, SCREENSHOTS and SETTINGS"
  />
</template>
