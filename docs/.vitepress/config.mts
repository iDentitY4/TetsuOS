import { defineConfig } from 'vitepress'

const repo = 'https://github.com/iDentitY4/TetsuOS'

// Served from GitHub Pages at https://identity4.github.io/TetsuOS/. DOCS_BASE overrides it,
// such as '/' for a custom domain.
const base = process.env.DOCS_BASE ?? '/TetsuOS/'

export default defineConfig({
  title: 'Tetsu OS',
  description: 'A Linux distribution for retro handhelds, running on the Anbernic RG35XX SP.',
  lang: 'en-US',
  base,
  cleanUrls: true,
  lastUpdated: true,
  // Dark first, like the launcher; the switch in the header still offers light
  appearance: 'dark',
  // The repository's own notes, read on GitHub rather than published as pages
  srcExclude: ['**/README.md'],
  head: [
    ['link', { rel: 'icon', type: 'image/png', href: `${base}logo.png` }],
    ['meta', { name: 'theme-color', content: '#4fd1c5' }],
  ],
  markdown: {
    // Feature notes and code use <angle> placeholders in prose, such as saves/<game>.srm
    html: false,
  },
  themeConfig: {
    logo: { src: '/logo.png', alt: '' },
    nav: [
      { text: 'Guide', link: '/guide/', activeMatch: '/guide/' },
      { text: 'Reference', link: '/reference/systems', activeMatch: '/reference/' },
      { text: 'Developers', link: '/developer/', activeMatch: '/developer/' },
      { text: 'Download', link: '/#download' },
    ],
    sidebar: {
      '/guide/': [
        {
          text: 'Getting started',
          items: [
            { text: 'What is Tetsu OS?', link: '/guide/' },
            { text: 'Requirements', link: '/guide/requirements' },
            { text: 'Installation', link: '/guide/installation' },
            { text: 'Adding games', link: '/guide/adding-games' },
            { text: 'Controls', link: '/guide/controls' },
          ],
        },
        {
          text: 'Using Tetsu OS',
          items: [
            { text: 'The launcher', link: '/guide/library' },
            { text: 'Playing games', link: '/guide/playing' },
            { text: 'RetroAchievements', link: '/guide/achievements' },
            { text: 'Wi-Fi', link: '/guide/wifi' },
            { text: 'Bluetooth', link: '/guide/bluetooth' },
            { text: 'Power and battery', link: '/guide/power-and-battery' },
            { text: 'Save backup', link: '/guide/save-backup' },
          ],
        },
        {
          text: 'Maintenance',
          items: [
            { text: 'Upgrading', link: '/guide/upgrading' },
            { text: 'Troubleshooting', link: '/guide/troubleshooting' },
            { text: 'Testing your device', link: '/guide/testing' },
            { text: 'FAQ', link: '/guide/faq' },
          ],
        },
        {
          text: 'Reference',
          collapsed: true,
          items: [
            { text: 'Supported systems', link: '/reference/systems' },
            { text: 'Settings', link: '/reference/settings' },
            { text: 'Files on TETSU-DATA', link: '/reference/files' },
          ],
        },
      ],
      '/reference/': [
        {
          text: 'Reference',
          items: [
            { text: 'Supported systems', link: '/reference/systems' },
            { text: 'Settings', link: '/reference/settings' },
            { text: 'Files on TETSU-DATA', link: '/reference/files' },
          ],
        },
        {
          text: 'Guide',
          collapsed: true,
          items: [
            { text: 'Installation', link: '/guide/installation' },
            { text: 'Playing games', link: '/guide/playing' },
            { text: 'Troubleshooting', link: '/guide/troubleshooting' },
          ],
        },
      ],
      '/developer/': [
        {
          text: 'Developers',
          items: [
            { text: 'Overview', link: '/developer/' },
            { text: 'Building from source', link: '/developer/building' },
            { text: 'Device access', link: '/developer/device-access' },
            { text: 'Boot and updates', link: '/developer/boot' },
            { text: 'The launcher', link: '/developer/launcher' },
            { text: 'Hardware notes', link: '/developer/hardware' },
            { text: 'Making a release', link: '/developer/releases' },
            { text: 'Contributing', link: '/developer/contributing' },
          ],
        },
        {
          text: 'Engineering log',
          items: [{ text: 'Feature notes', link: '/features' }],
        },
      ],
      '/features': [
        {
          text: 'Developers',
          items: [
            { text: 'Overview', link: '/developer/' },
            { text: 'Building from source', link: '/developer/building' },
            { text: 'Hardware notes', link: '/developer/hardware' },
            { text: 'Feature notes', link: '/features' },
          ],
        },
      ],
    },
    socialLinks: [{ icon: 'github', link: repo }],
    editLink: {
      pattern: `${repo}/edit/main/docs/:path`,
      text: 'Edit this page on GitHub',
    },
    search: { provider: 'local' },
    outline: { level: [2, 3] },
    footer: {
      message: 'Tetsu OS is released under the MIT license. Emulator cores keep their own licences.',
      copyright: 'Copyright © 2026 iDentitY4',
    },
  },
})
