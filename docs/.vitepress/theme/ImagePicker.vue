<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { withBase } from 'vitepress'
import { devices, REPO } from './devices'

// The home page's download box: pick a device, get its card image from the newest
// release. The release is asked from GitHub's API in the browser, so the docs never need
// a new version written in; without an answer it links to the Releases page.
interface Asset {
  name: string
  size: number
  browser_download_url: string
}
interface Release {
  tag_name: string
  html_url: string
  assets: Asset[]
}

const deviceName = ref(devices[0].name)
const newerScreen = ref(false)
const release = ref<Release | null>(null)
const failed = ref(false)

const device = computed(() => devices.find((d) => d.name === deviceName.value) ?? devices[0])
const rev6 = computed(() => !!device.value.rev6 && newerScreen.value)
const version = computed(() => release.value?.tag_name ?? '<version>')
const fileName = computed(
  () => `tetsu-${device.value.name}${rev6.value ? '-rev6' : ''}-${version.value}-sdcard.img.gz`,
)
const asset = computed(() => release.value?.assets.find((a) => a.name === fileName.value))
const sums = computed(() => release.value?.assets.find((a) => a.name.startsWith('SHA256SUMS')))
const releasesPage = `https://github.com/${REPO}/releases/latest`
const megabytes = (bytes: number) => `${Math.round(bytes / 1_000_000)} MB`

onMounted(async () => {
  // ?device=rg40xx-h picks one, for links from elsewhere
  const wanted = new URLSearchParams(window.location.search).get('device')
  if (wanted && devices.some((d) => d.name === wanted)) deviceName.value = wanted
  try {
    const response = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
      headers: { Accept: 'application/vnd.github+json' },
    })
    if (!response.ok) throw new Error(`GitHub answered ${response.status}`)
    release.value = (await response.json()) as Release
  } catch {
    failed.value = true
  }
})
</script>

<template>
  <section id="download" class="image-picker">
    <h2>Download</h2>
    <p class="lead">Pick your device for its card image.</p>

    <div class="row">
      <label for="picker-device">Device</label>
      <select id="picker-device" v-model="deviceName">
        <option v-for="d in devices" :key="d.name" :value="d.name">
          {{ d.label }}{{ d.tested ? '' : ' (untested)' }}
        </option>
      </select>
    </div>

    <fieldset v-if="device.rev6" class="row">
      <legend>Screen</legend>
      <label><input v-model="newerScreen" type="radio" :value="false" /> The first one</label>
      <label><input v-model="newerScreen" type="radio" :value="true" /> Newer (board revision 6)</label>
      <p class="hint">Not sure? Try the first. If the screen stays dark, write the newer one.</p>
    </fieldset>

    <p v-if="device.tested" class="status ok">Tested on a real {{ device.label }}.</p>
    <p v-else class="status warn">
      Nobody has started Tetsu OS on a {{ device.label }} yet. It may not start, or start with
      things that don't work. Trying does no harm, and
      <a :href="withBase('/guide/testing')">a report</a> helps.
    </p>
    <p v-if="device.note" class="hint">{{ device.note }}</p>

    <div class="file">
      <code>{{ fileName }}</code>
      <span v-if="asset" class="size">{{ megabytes(asset.size) }}</span>
    </div>

    <div class="actions">
      <a v-if="asset" class="button brand" :href="asset.browser_download_url">Download {{ version }}</a>
      <a v-else class="button brand" :href="release?.html_url ?? releasesPage">Open the release</a>
      <a v-if="sums" class="button alt" :href="sums.browser_download_url">Checksums</a>
      <a class="button alt" :href="withBase('/guide/installation#_2-write-the-card')">How to write it</a>
    </div>

    <p v-if="failed" class="hint">
      GitHub didn't answer, so the version is missing above: take the file with this name from
      the newest release.
    </p>
    <p v-else-if="release && !asset" class="hint">
      {{ version }} has no image for this device: see the release for its files.
    </p>
    <p class="hint">
      Write it with balenaEtcher or Raspberry Pi Imager, unzipped or not. Writing erases the
      card; games and saves go on its TETSU-DATA part afterwards. Already on v0.1? Write the image
      instead of using SYSTEM UPDATE (<a :href="withBase('/guide/upgrading')">why</a>).
    </p>
  </section>
</template>

<style scoped>
.image-picker {
  max-width: 640px;
  margin: 48px auto;
  padding: 24px;
  border: 3px solid var(--vp-c-brand-1);
  background: var(--vp-c-bg-soft);
}

@media (max-width: 688px) {
  .image-picker {
    margin: 32px 16px;
    padding: 20px 16px;
  }
}

h2 {
  margin: 0;
  font-family: var(--tetsu-pixel);
  font-size: 24px;
  font-weight: 400;
  text-transform: uppercase;
  color: var(--vp-c-brand-1);
  border: none;
  padding: 0;
}

.lead {
  margin: 4px 0 16px;
  color: var(--vp-c-text-2);
}

.row {
  margin: 0 0 16px;
  padding: 0;
  border: none;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 16px;
}

.row > label:first-child,
legend {
  font-weight: 600;
  min-width: 64px;
  float: left;
}

select {
  flex: 1 1 220px;
  min-width: 0;
  padding: 8px 10px;
  border: 2px solid var(--vp-c-divider);
  border-radius: 0;
  background: var(--vp-c-bg);
  color: var(--vp-c-text-1);
  font: inherit;
  appearance: auto;
}

fieldset label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  cursor: pointer;
}

.hint {
  margin: 8px 0 0;
  width: 100%;
  font-size: 14px;
  color: var(--vp-c-text-2);
}

.status {
  margin: 0;
  padding: 8px 12px;
  font-size: 14px;
}

.status.ok {
  background: var(--vp-c-tip-soft);
  box-shadow: inset 3px 0 var(--vp-c-brand-1);
}

.status.warn {
  background: var(--vp-c-warning-soft);
  box-shadow: inset 3px 0 var(--vp-c-warning-1);
}

.file {
  margin: 16px 0 0;
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 4px 12px;
}

.file code {
  overflow-wrap: anywhere;
  font-size: 14px;
}

.size {
  font-size: 14px;
  color: var(--vp-c-text-2);
}

.actions {
  margin-top: 16px;
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.button {
  display: inline-block;
  padding: 0 18px;
  line-height: 36px;
  font-family: var(--tetsu-pixel);
  font-size: 13px;
  text-transform: uppercase;
  text-decoration: none;
  border: 3px solid transparent;
  transition: background-color 0.25s;
}

.button.brand {
  background: var(--vp-button-brand-bg);
  color: var(--vp-button-brand-text);
}

.button.brand:hover {
  background: var(--vp-button-brand-hover-bg);
  color: var(--vp-button-brand-hover-text);
}

.button.alt {
  background: var(--vp-c-bg);
  color: var(--vp-c-text-1);
  border-color: var(--tetsu-line);
}

.button.alt:hover {
  border-color: var(--vp-c-brand-1);
  color: var(--vp-c-brand-1);
}

a:not(.button) {
  color: var(--vp-c-brand-1);
  text-decoration: underline;
}
</style>
