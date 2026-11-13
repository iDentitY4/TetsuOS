# Supported systems

Each console gets a tile on the home screen once it has games. The launcher picks the
console by the file's extension, so folder names don't matter. **No BIOS files are needed**
for any of them.

## Consoles

| Console | Extensions | Emulator (core) | Cheats | Bezel |
| --- | --- | --- | :---: | :---: |
| Atari 2600 | `.a26` | Stella | | |
| Vectrex | `.vec` | vecx | | |
| NES / Famicom | `.nes` | FCEUmm | ✓ | |
| SG-1000 | `.sg` | Genesis Plus GX | ✓ | |
| Master System | `.sms` | Genesis Plus GX | ✓ | |
| Atari 7800 | `.a78` | ProSystem | | |
| PC Engine / TurboGrafx-16 | `.pce` | Beetle PCE Fast | ✓ | |
| Mega Drive / Genesis | `.md` `.gen` `.smd` | Genesis Plus GX | ✓ | |
| Game Boy | `.gb` | mGBA | ✓ | ✓ |
| Atari Lynx | `.lnx` `.lyx` | Handy | | ✓ |
| Game Gear | `.gg` | Genesis Plus GX | ✓ | ✓ |
| SNES / Super Famicom | `.sfc` `.smc` | Snes9x | ✓ | |
| Watara Supervision | `.sv` | Potator | | ✓ |
| 32X | `.32x` | PicoDrive | ✓ | |
| PlayStation | `.cue` `.chd` `.pbp` `.m3u` | PCSX ReARMed | ✓ | |
| Virtual Boy | `.vb` `.vboy` | Beetle VB | | |
| Tamagotchi (P1) | `tama.b` | tamalib | | |
| Game Boy Color | `.gbc` | mGBA | ✓ | ✓ |
| Neo Geo Pocket | `.ngp` | Beetle NeoPop | | ✓ |
| Neo Geo Pocket Color | `.ngc` `.ngpc` | Beetle NeoPop | | ✓ |
| WonderSwan | `.ws` | Beetle Cygne | ✓ | ✓ |
| WonderSwan Color | `.wsc` | Beetle Cygne | ✓ | ✓ |
| Game Boy Advance | `.gba` | mGBA | ✓ | ✓ |
| Pokémon Mini | `.min` | PokeMini | | ✓ |
| Arduboy | `.hex` | Arduous | | ✓ |

Cartridge games also work inside `.zip` files (not PlayStation). **Bezel** is what the
AUTO border draws; any console can choose a bezel in DISPLAY > BORDER.

## Games

| Game | Files | Engine |
| --- | --- | --- |
| Doom | `.wad` (Freedoom: Phase 1 and 2 included) | PrBoom |
| Cave Story | `Doukutsu.exe` with its `data` folder (included) | NXEngine |

## Notes per system

**PlayStation.** The list shows only the `.cue`; keep the `.bin` files beside it. Discs of a
multi-disc game group by name, or list them in an `.m3u`. Games are shown at 4:3. Most
European games run at 50 Hz, so some frames show twice on the 60 Hz screen. Vibration
isn't supported yet. RetroAchievements can't read `.chd` files.

**Mega Drive.** Rename `.bin` files to `.md`. CD and 32X CD games aren't supported. The
Mega Drive, Master System and PC Engine show at 4:3, as their pixels aren't square.

**WonderSwan.** The WonderSwan has no Select button, so Select rotates the picture for
vertical games.

**Lynx.** Games rotate by their cartridge header, and Select overrides it.

**Game Boy.** Super Game Boy borders are off, so enhanced games fill the screen. LCD
GHOSTING imitates the original screen's blur.

**Neo Geo Pocket, Lynx and Pokémon Mini** save to their own files (`.flash`, `.eeprom`,
`.eep`) in `saves` when you leave the game.

**Atari 2600 and 7800, Vectrex and Supervision** have no save memory, so their games keep
only the resume point and save states.

**Vectrex and Virtual Boy** run at 50 Hz. The Vectrex's built-in game, Mine Storm, runs
with an empty cartridge. The Virtual Boy shows red on black.

**Arduboy** runs at about 57 fps on BALANCED; set
[PERFORMANCE](../guide/power-and-battery#performance) for full speed.

**Doom and Cave Story** have no save states or resume. Save from their own menus. R2 is
Doom's own button rather than fast-forward.

## Licences

The emulators are downloaded at build time under their own licences. Snes9x and Genesis
Plus GX forbid commercial use, which matters for anyone selling devices with Tetsu OS.
