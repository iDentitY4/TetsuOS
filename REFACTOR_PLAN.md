# Refactor plan: `src/launcher`

Baseline (2026-11-09, commit 11ac9a1): `cargo check` clean, 353 tests pass, default clippy
12 warnings (fixed in batch 3), `clippy::pedantic` 336 warnings (72 in `libretro.rs`, 31 `achievements.rs`, 28 `sounds.rs`).

## 1. Map

One crate, one binary: `launcher` 0.1.0 (edition 2021, ~22.4k lines in 50 modules, flat
`src/*.rs`). No workspace, no features, no lib target. `build.rs` compiles the vendored
`rcheevos/` C sources with `cc`. Release: `opt-level=s`, LTO, `panic=abort`.

**Entry points** (`main.rs:76`, chosen from the arguments):
no args → `app::run_launcher` (menus) · `--play` → `libretro::play` (one game per process) ·
`--render` → `libretro::render` · `--core-options` → `libretro::print_core_options` ·
`--jingle` → `sounds::boot_jingle` · `--preview*` → `previews.rs` (PPM on the PC).

**Layers** (intended direction, top depends on bottom):

| Layer | Modules |
|---|---|
| Modes | `main`, `app`, `setup`, `game`, `previews` |
| Screens/flows | `launcher` (button logic), `screens`, `wifi_screen`, `wifi_menu`, `boxart_screen`, `core_options_screen`, `keyboard`, `pause`, `corner`, `usb_drive`, `sleep` |
| Emulation | `libretro` (frontend, 2176 non-test lines), `achievements` (rcheevos FFI), `cheats`, `core_options`, `controls`, `screenshot`, `rom_zip` |
| Data | `library`, `systems`, `game_files`, `names`, `recent`, `favorites`, `play_time`, `boxart`, `boxart_fetch` |
| Device | `fb`, `input`, `sounds`, `wifi`, `ssh`, `power`, `performance`, `clock`, `boot_partition`, `system_info` |
| Base | `canvas`, `font`, `icons`, `png`, `settings`, `i18n`, `storage`, `repeat` |

**Actual direction:** 24 of the 50 modules form one dependency cycle (`canvas`, `fb`, `png`,
`settings`, `i18n`, `input`, `screens`, `libretro`, `achievements`, ...); `systems` ⇄ `rom_zip`
is a second. `main.rs:55-67` glob-re-exports 12 modules and 12 modules `use crate::*`.

**Unsafe / FFI / syscalls** (113 `unsafe` uses in 5 files):
- `libretro.rs` (58): `dlopen` of cores via `libloading` (leaked, never unloaded), six
  `extern "C"` callbacks, core function pointers, raw slices from core memory. State in a
  thread-local `HOST: RefCell<Option<Host>>`.
- `achievements.rs` (49): hand-written rcheevos FFI (`mod ffi`, line 61-330), seven callbacks,
  thread-locals `STATE`/`MEMORY_MAP`/`CORE_MEMORY`; HTTP on spawned threads, answers returned
  to the core thread over a channel.
- `fb.rs` (4): fbdev `ioctl`s (`FBIOGET_*`, `FBIO_WAITFORVSYNC`, `KDSETMODE`), `u32`→bytes cast.
- `wifi.rs:320` `libc::kill`; `system_info.rs:169` reads the ARM counter registers (`asm!`).
- Signals: `signal-hook` flag in `libretro.rs:34`. Processes: `aplay`, `wpa_cli`, `udhcpc` etc.

**Platform-specific code:** one cfg pair, `system_info.rs:165/176` (`aarch64`). Everything else
assumes Linux, fbdev, evdev and little-endian without gates; ~65 absolute paths (`/mnt/data`,
`/sys`, `/proc`, `/dev`, `/usr/lib/libretro`) are spread over 25 modules.

**Threads:** input reader (`input.rs`), box art and cheat downloads, achievements HTTP,
Wi-Fi join (`app.rs:856`), sound (`sounds.rs:216`). Shared state: `Mutex` statics in
`settings`, `sounds`, `rom_zip`, `core_options`, `boot_partition`.

## 2. Findings

Effort: S < 1 h, M half a day, L more. "Skip" marks fixes that would change behaviour.

### 2.1 Soundness
| # | Where | Problem | Fix | Effort |
|---|---|---|---|---|
| S1 | `libretro.rs:562` | `pitch * (height - 1)` underflows when a core sends `height == 0` (wraps in release → huge slice, UB) | return early when `width == 0 \|\| height == 0` | S |
| S2 | `libretro.rs:599` | `from_raw_parts(data, frames*4)` with null `data` is UB even for 0 frames | return `frames` early if `data.is_null() \|\| frames == 0` | S |
| S3 | `libretro.rs:287-299`, `:529` | `DiskControl` holds non-nullable `fn` pointers copied from the core; a null one is instant UB, and `data` isn't null-checked | make fields `Option<fn>`, reject the interface if a required one is `None` or `data` is null | S |
| S4 | `libretro.rs:437-535` | `environment` dereferences `data` unchecked for the `GET_*`/`SET_PIXEL_FORMAT` commands; no `# Safety` docs | one null check at the top for commands that need `data`; SAFETY comment per group | S |
| S5 | `libretro.rs:309,317,327,437,542,591,597,606`; `achievements.rs:466,475,482` | unsafe blocks and callbacks without SAFETY comments (19 comments for 58 uses in `libretro.rs`, 28 for 49 in `achievements.rs`) | add comments stating the invariant; `#![deny(clippy::undocumented_unsafe_blocks)]` | M |
| S6 | `libretro.rs:392`, `achievements.rs:426,476` | callbacks `borrow_mut` a `RefCell`; a core re-entering during a borrow panics → abort inside C frames. Safe today only by convention ("copied out before calling") | document the rule at `with_host`/`with_state`; add `debug_assert!(try_borrow_mut().is_ok())` before each call into the core | S |
| S7 | `libretro.rs:1027` | `save_ram(&mut self) -> &mut [u8]` hands out core memory whose lifetime is tied to `self`, not to the loaded game; a core may also remap it | keep, but document; never hold across `run_frame` (already true) | S |
| S8 | `fb.rs:231` | `u32`→`u8` slice assumes little-endian; only a comment guards it | take the fast path only under `cfg!(target_endian = "little")` | S |
| S9 | `achievements.rs:61` | blanket `#[allow(dead_code)]` on the FFI module hides unused/stale declarations; struct layouts are unchecked against the C headers | drop the blanket allow, mark single items; add `size_of` const asserts for the structs read by value (test at `:1658` already links C) | M |

### 2.2 Correctness
| # | Where | Problem | Fix | Effort |
|---|---|---|---|---|
| C1 | `rom_zip.rs:39,47` | `KNOWN.lock().unwrap()`: the only poisoning panics in non-test code | `unwrap_or_else(PoisonError::into_inner)`, as a cache needs no poisoning | S |
| C2 | `libretro.rs:1659` | `expect("checked above")` on `achievements_page` inside the 245-line `pause` | bind with `let Some(..) else` where it is checked | S |
| C3 | `libretro.rs:392` | `expect("host is set before the core runs")` in every callback | keep (a real invariant); covered by S6 | - |
| C4 | `achievements.rs:1313` | `cfg!(test)` branch and `#[cfg(test)] FAKE_SERVER` (`:499`) inside production functions | inject through one `#[cfg(test)]` seam function | S |
| C5 | `sounds.rs`, `wifi.rs`, `wifi_screen.rs`, `achievements.rs` (27 `let _ =`/`.ok();`) | most are deliberate and commented; a few (wifi, sounds) drop errors silently | audit; log with `eprintln!` where no comment explains it. **Adds log lines only** | S |
| C6 | `boot_partition.rs:139`, `controls.rs:74` | `Option<Option<T>>` state | small enum (`NotRead`/`Missing`/`Read(T)`) | S |
| C7 | `screens.rs:734,737,1555` etc. | pedantic nits that are real: manual midpoint, `&Option<T>` (`screens.rs:326`), by-value args not consumed (`libretro.rs:1874`, `input.rs:215`, `wifi_screen.rs:63`), needless `Option` returns (`achievements.rs:1005,1041`) | apply clippy's suggestions | S |

### 2.3 Architecture
| # | Where | Problem | Fix | Effort |
|---|---|---|---|---|
| A1 | `canvas.rs:5-13`, `:431-522` | the drawing primitive imports `achievements`, `cheats`, `controls`, `core_options`, `pause`, `screens`: in-game menu screens live in the base layer | move those `draw_*` functions (lines ~420-634) to a new `pause_screens.rs`; `canvas` keeps primitives only | M |
| A2 | `png.rs:5`, `canvas.rs:5` → `boxart::Image` | the base image type lives in a feature module | move `Image` to `canvas.rs` (or `image.rs`), re-export from `boxart` | S |
| A3 | `settings.rs:10-12` ⇄ `canvas`, `controls`, `clock`; `i18n` ⇄ `settings`; `input.rs:3` → `i18n` | base modules depend on each other and upward | move `Rgb` to its own `color.rs`; pass the language into `i18n` at start instead of reading settings; move `input`'s translated labels to `screens` | M |
| A4 | `systems.rs:4` → `libretro::{CoreSetup, Persistence}`; `systems` ⇄ `rom_zip` | console table depends on the frontend | move `CoreSetup`/`Persistence` to `systems.rs`; move `system_for_extension` use behind a parameter in `rom_zip` | S |
| A5 | `libretro.rs` (2176 lines) | god module: FFI types, callbacks, scaling/effects (`~640-900`), save files (`1118-1160`), audio (`1208-1270`), the pause loop (`1593-1860`), `play` (228 lines, 8 args) | split into `libretro/{ffi,host,core,scale,save,audio,menu,play}.rs`, moves only; then cut `pause`/`play` into steps | L |
| A6 | `main.rs:55-67` + 12× `use crate::*` | glob re-exports hide dependencies and made the cycle invisible | replace with explicit `use crate::module::Item` (clippy lists all 24 sites) | M |
| A7 | `app.rs:135,418,881`, `wifi_screen.rs:78`, `launcher.rs:881` | 110-180 line functions | extract per-action helpers after A6 | M |
| A8 | `achievements.rs:515`, `boxart_fetch.rs:69` | `ureq` agent configured in two places (not yet compared line by line) | one `http.rs` with `agent(timeouts)`; keep each caller's exact settings | S |
| A9 | 25 modules | ~65 literal device paths | one `paths.rs` with the constants (`DATA`, `SETTINGS`, `CORES`, ...); same values | M |

### 2.4 Portability
| # | Where | Problem | Fix | Effort |
|---|---|---|---|---|
| P1 | crate-wide | Linux-only (`evdev`, fbdev, `std::os::unix`) with no gate; builds elsewhere fail deep inside | `#[cfg(not(target_os = "linux"))] compile_error!` in `main.rs` | S |
| P2 | `libretro.rs`, `achievements.rs`, `sounds.rs` | 86 `as` casts flagged (truncation/sign/wrap), some on values from C (`index as c_uint`, sizes) | `try_from` / `From` where the source is external; `#[expect]` with a reason where the range is known | M |
| P3 | `libretro.rs:931` | `/tmp` as the system directory for `--core-options` | `std::env::temp_dir()` — same on the device | S |
| P4 | `system_info.rs:165` | the only arch gate; fine | none | - |

### 2.5 Idiom and maintainability
| # | Where | Problem | Fix | Effort |
|---|---|---|---|---|
| I1 | crate-wide | 654 `pub` items in a binary crate vs 224 `pub(crate)`; `pub` means nothing here and hides dead code from the compiler's lint | `pub` → `pub(crate)` mechanically, then delete what `dead_code` reports (report first if anything looks intended) | M |
| I2 | crate-wide | errors are `io::Error::other(String)` everywhere; fine for a binary. No `thiserror`/`anyhow` needed | none; skip | - |
| I3 | 26 sites in `libretro.rs`/`achievements.rs` | `as *const T` pointer casts | `.cast::<T>()`, `&raw const` | S |
| I4 | 55 sites | doc comments without backticks; 9 unseparated literals; 7 redundant closures; 6 single-arm `match`; 5 `format!` in iterator chains | `cargo clippy --fix` for the machine-applicable ones, review the diff | S |
| I5 | `settings.rs:350` | `Settings` has more than 3 bools | leave: they map 1:1 to file lines | - |
| I6 | `Cargo.toml` | no `[lints]` table, so the bar isn't recorded | add `[lints.clippy]` with the pedantic subset that is clean after the batches | S |

### 2.6 Build
| # | Where | Problem | Fix | Effort |
|---|---|---|---|---|
| B1 | `Cargo.toml` | all 10 dependencies are used (grep; `machete`/`udeps` not installed) | none | - |
| B2 | `cargo tree -d` | duplicates `getrandom` 0.2/0.4, `miniz_oxide` 0.8/0.9, `syn` 2/3: all transitive (`ring`, `png` vs `flate2`, `zeroize_derive`) | none possible without upstream bumps; **skip** | - |
| B3 | `Cargo.toml` | `serde` + `serde_json` only parse two small JSON answers (`cheats.rs`, `boxart_fetch.rs`) | leave; replacing them risks behaviour | - |
| B4 | `build.rs:8-9` | `expect` with a vague message; `rerun-if-changed` on the whole directory is right | name the directory in the message | S |

## 3. Batches

Each ends with `cargo check`, `cargo clippy --all-targets`, `cargo test`, and one commit.
None changes settings files, save formats, arguments, or what is drawn. Device builds are
not part of a batch; a `launcher-rebuild` and a push for a smoke test are suggested after
batches 1, 5 and 7.

- [x] **1. FFI guards** — S1, S2, S3, S4, S8, C1, C2. Only changes what happens on input that is UB or a panic today.
- [x] **2. Unsafe documentation** — S5, S6, S7, S9, I3. Comments, `debug_assert`s, pointer-cast spelling, `deny(undocumented_unsafe_blocks)`.
- [x] **3. Clippy mechanical** — C6, C7, I4, B4. No moves. Left: `play`'s 8 arguments and its by-value `cheat_source` (batch 8), P3 (with the paths, batch 9), `controls.rs:74` (the inner `Option` is the target "none"), `&self` on `in_flight`/`memory_found`, and the lints whose fixes read worse here (`single_match_else`, `manual_let_else`, `format_push_string`).
- [x] **4. Layering, small moves** — A2 (`image.rs`), A4 (`CoreSetup`, `Persistence` → `systems.rs`), A8 (`http.rs`), and `Rgb` → `color.rs` from A3. Not done, on a closer look: C4 (the `#[cfg(test)]` fake server and the one `cfg!(test)` are short and clear as they are) and `systems` ⇄ `rom_zip` (breaking it means a predicate through 11 call sites, for a relation that is mutual by nature).
- [ ] **5. `canvas` split** — A1: in-game menu drawing out of `canvas.rs`.
- [ ] **6. Explicit imports and visibility** — A6, I1, rest of A3, P1.
- [ ] **7. `libretro` split** — A5 as file moves first (one commit), function extraction second (another).
- [ ] **8. Long functions** — A7.
- [ ] **9. Paths and casts** — A9, P2, then I6 to record the lint bar.

Flagged, not planned: C5 (adds log lines), B2/B3 (dependency changes), and any change to
`Settings` fields (I5).
