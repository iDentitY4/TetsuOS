# Contributing

Issues and pull requests are welcome at
[github.com/iDentitY4/TetsuOS](https://github.com/iDentitY4/TetsuOS).

## Before a pull request

- `cargo test` and `cargo clippy --all-targets` pass in `src/launcher/`.
- Kernel and package patches apply with Buildroot's `support/scripts/apply-patches.sh`.
- Say what was tested on a real device and what wasn't. Sound, feel and anything physical,
  like the lid, need a person holding the device.
- Add the feature's details to [Feature notes](../features) (how it works, what was measured,
  how it was tested) and update the user guide here.

## Translations

The launcher and in-game menu are translated through `src/launcher/lang/`. A language is:

1. a file such as `lang/fr.txt`, in lines of `ENGLISH = TRANSLATION`, with the same
   `[max N]` sections as `de.txt`
2. a variant in `settings::Language`
3. a line in `i18n::table`

Everything is in capitals in a fixed-width font, so keep within each section's length. The
font has A–Z, digits, common symbols and Ä, Ö, Ü and ß; other accented capitals need glyphs in
`ui/font.rs`. `cargo test i18n` reports what is missing, unused or too long. The WI-FI
TRANSFER page has its own small table in `transfer_page.html`.

## Adding a console

1. A Buildroot package in `package/libretro-<core>/` built from a pinned commit, added to
   `external.mk`, `Config.in` and `configs/common.config`.
2. An entry in `SYSTEMS` in `src/launcher/src/library/systems.rs`: its key, name, extensions,
   core, persistence, aspect, libretro-thumbnails folder, cheat folder and RetroAchievements
   console ID. The order of `SYSTEMS` is the order of the tiles.
3. A pictogram in `ui/icons.rs`, and a bezel colour in `libretro/border.rs` for a handheld.
4. Check `launcher --core-options` for the core's options, and whether it needs any
   `FRONTEND_DEFAULTS`.
5. Test on the device: speed (`/var/log/launcher.log` has fps and underruns), save states,
   resume and its save files.

Avoid extensions another console already uses (such as `.bin`): the launcher decides the
console by extension alone.

## Adding a setting

1. A field in `Settings`, with `choices!` for fixed choices, and lines in `parse` and `format`.
2. A row in the group's `Group::settings` in `launcher.rs`, within `MAX_GROUP_SETTINGS`.
3. Its labels through `i18n::tr`, with German lines.
4. If an init script reads it, use `/usr/libexec/setting KEY DEFAULT`, which falls back to
   `settings-backup.txt`.
5. Decide whether RESET SETTINGS resets it.

## Documentation

This site is [VitePress](https://vitepress.dev). Pages are Markdown in `docs/`:

```bash
npm install
npm run docs:dev      # live preview at http://localhost:5173/
npm run docs:build    # static site in docs/.vitepress/dist
```

The sidebar is in `docs/.vitepress/config.mts`. Screenshots in `docs/public/screenshots/` are
made with the launcher's `--preview*` flags and converted to PNG:

```bash
cd src/launcher
cargo run --release -- --preview-list /tmp/list.ppm
magick /tmp/list.ppm ../../docs/public/screenshots/list.png
```

The site's look follows the launcher's: `docs/.vitepress/theme/custom.css` sets its colours,
square tiles and the Silkscreen pixel font for headings (self-hosted from
`@fontsource/silkscreen`). The home page's feature icons are the launcher's own, drawn as
SVG from `src/launcher/src/ui/icons.rs` by `scripts/docs-icons.py`; run it again after
changing one, or with names (`scripts/docs-icons.py WIFI DOOM`) for others.

Pushes to `main` that change `docs/` publish the site to GitHub Pages through
`.github/workflows/docs.yml`. It is served at [tetsu-os.org](https://tetsu-os.org), a
custom domain set in the repository's Settings > Pages; the old
`identity4.github.io/TetsuOS/` address redirects there.
