# Features in detail

How each part of Tetsu OS was built, measured and tested, moved out of
`CLAUDE.md` on 2026-11-08 to keep that file short. Read the part you work on. Notes on who
confirmed what are as of the date given; `CLAUDE.md` keeps the current list of what still
needs checking.

Working on the test unit, confirmed by the user:

- Boots to the launcher
- Power button starts the device, and a short press shuts it down
- USB-C root console
- Boot log saved at about 15 and 60 seconds after init starts
- The button test shows each control as it is pressed: filled while held, outlined
  once seen, with the last press named below. An unlisted key shows as `KEY <code>`.
- Audio: speakers and headphones play, and outputs switch automatically with the jack,
  including during playback and when booting with headphones plugged in
- USB console connects without `Login incorrect`, using `getty -w`, so the login prompt
  appears only after pressing Enter
- GPU: OpenGL ES 3.1 through Panfrost on the Mali-G31. `kmscube` ran at 59.94 fps with
  no visible tearing. The GPU runs at a fixed 432 MHz, because there is no clock scaling
  table yet

- The launcher lists games from `/mnt/data/roms` and the folders inside it, in any
  arrangement, as the file extension decides the console; hidden files and folders are
  left out. It has the button test as a menu entry and working volume buttons
- A libretro frontend in the same app, in `libretro/`, runs the mGBA core from
  `package/libretro-mgba`. Pokemon FireRed runs at 59.7 fps with clean sound and no tearing,
  at about 50 C. Holding Menu + Start leaves a game. `/var/log/launcher.log` has timings
- Games and saves are in `roms` and `saves` on the TETSU-DATA partition. They survive leaving a
  game, rebooting and reflashing with `flash.sh`
- CPU speed scaling, up to 1512 MHz on this chip. It was fixed at 1008 MHz before, which
  gave only 43 fps in the game

- Tamagotchi P1 emulation through `package/libretro-tamalib`, using `tama.b` in `roms`, at
  60 fps with the CPU at its lowest 480 MHz. The launcher picks a core by file extension.
  The pet's whole state is saved once a minute, on leaving and at power-off, in
  `saves/tama.state`, and it pauses while the game is off

- Lid sleep, using `s2idle` because TF-A has no deep sleep, from the menu and in games.
  Sound stops in sleep (patch 0011) and resumes on opening. A power button press with the
  lid closed doesn't shut down (patch 0010), and the device goes back to sleep. The
  Tamagotchi carries on correctly after sleep
- Power-off after a long lid sleep (2026-11-10): after 2 hours of lid sleep on battery
  (Settings > SYSTEM > POWER OFF AFTER SLEEP: 1, 2 (default), 4 or 8 HOURS or NEVER,
  saved as `sleep_power_off=`),
  an RTC alarm (`/sys/class/rtc/rtc0/wakealarm`, set to the time left before each
  `freeze`) wakes the device, and it powers off, so s2idle doesn't drain the battery.
  A game in progress saved its resume state when the lid closed, and saves again on the
  shutdown's SIGTERM; `playing.txt` stays, so it resumes at the next start. On USB power
  (status other than `Discharging`) it keeps sleeping for that long again instead, as the
  battery doesn't drain and USB power may start the device again. The time counts with
  `CLOCK_BOOTTIME`, as `Instant` stops in s2idle. Waking for a power button press doesn't
  reset it. NEVER sets no alarm. `LAUNCHER_LONG_SLEEP` overrides the setting in seconds,
  for testing. Claude checked on the
  device that a `+40` alarm wakes s2idle, that the boot clock advanced 41.7 s across it and
  that the alarm clears once it fires. The user confirmed on 2026-11-10, with
  `LAUNCHER_LONG_SLEEP=120` and the cable unplugged, that the device powered off with the
  lid closed during a game and that the game resumed at the next start. The setting is
  the ninth row of SYSTEM, the only group with that many (`MAX_GROUP_SETTINGS`), which
  leaves room for one note line: its note about the power button. Claude checked on the
  device that the row shows 2 HOURS, Right saves `4h`, and the note fits under the rows
  with SLEEP WHEN IDLE on; a sleep with a setting other than 2 hours hasn't been tried
- Shutdown screen (2026-11-10): a power symbol in the accent colour above POWERING OFF
  (SCHALTET AUS), or REBOOTING or BATTERY EMPTY, stays on the screen until the device is
  off (`screens::shutdown_screen`, preview with `--preview-shutdown`). The power button's
  acpid script (`/etc/acpi/PWRF/00000080`) sends SIGUSR1 to every `launcher` process,
  waits 0.2 s and runs `poweroff` as before. The menus' process and a game's both listen
  (`app::show_shutdown_on_signal`, a `signal_hook` thread), as either may be drawing; each
  draws the screen with `Framebuffer::draw_last`, after which `fb.rs` leaves out every
  write of that process, so a game's next frame or a menu redraw can't replace it. A
  game stops running frames until the shutdown's SIGTERM, which saves its resume state as
  before, so it resumes at the next start. The framebuffer keeps its picture after the
  launcher stops: Claude checked on the device that the menu still showed 2 s after
  `S90launcher stop`, that `killall -USR1 launcher` showed the screen, and that a press
  afterwards left it as it was. Not yet tried: a real power button press, one during a
  game, and the menu's POWER OFF and REBOOT with the new screen
- The battery level in the menu
- Updating over USB. The gadget is serial plus CDC Ethernet, set up through configfs by
  `/usr/sbin/usb-gadget start` from `S41usbnet`, with the fixed MACs and the USB ID
  (`0525:a4aa`) that the `g_cdc` gadget used before. `S41usbnet` gives the device `10.35.35.1`, and `udhcpd` gives the
  PC `10.35.35.2` with no router option. Over USB, dropbear allows root's empty
  password (`-B`); Settings > ADVANCED > SSH chooses where it listens (see below).
  `scripts/push.sh {app|rootfs|boot|all}` copies output over SSH with rsync
- USB STORAGE, in Settings > DATA, shows the card's TETSU-DATA partition to the PC as
  a USB drive, so games can be copied without taking the card out. A drive is built in
  on Windows, macOS and Linux, unlike MTP, and needs no program on the device.
  `usb-gadget storage` syncs and unmounts `/mnt/data`, gives the partition to the
  kernel's mass storage function as a removable drive, and binds Ethernet plus the drive
  under another ID (`0525:a4a5`), because Windows remembers a device's interfaces by
  its ID. The serial port is left out for the endpoints, so SSH keeps working. The PC
  sees the partition as a disk with no partition table, which all three read.
  Ejecting the drive on the PC empties the function's `lun.0/file`, which the screen
  checks twice a second, and leaves the screen. B also leaves, and asks again while the
  PC still has the drive. Leaving runs `usb-gadget network`: it closes the drive with
  `forced_eject`, binds serial plus Ethernet again, repairs the partition with
  `fsck.fat -a` (for a PC unplugged without ejecting), mounts it and the launcher reads
  the settings and games again. Meanwhile the device doesn't sleep: closing the lid only
  turns the screen off. Volume and brightness don't change, as they are saved on the
  card. An empty battery leaves the screen, mounts the card and powers off. Writes need
  patch 0014 (see the MUSB fact below). Mounting, copying and deleting games and ejecting
  were checked by the user on Linux on 2026-11-08; Windows and macOS haven't been tried
- TETSU-DATA is repaired at boot after an unclean shutdown (2026-11-12). `S15data` reads
  the FAT dirty bit (boot sector byte 65 on FAT32, 37 on FAT16) and runs `fsck.fat -a -w`
  only when it is set, logging to `/var/log/fsck-data.log`: 1.2 s on the 64 GB card,
  nothing on a clean boot. Without it, one power cut left directory entries pointing at
  clusters the FAT still had free (the card kept an older FAT sector, though the launcher
  `fsync`s before renaming), and the kernel, on reading such a file, made the partition
  read-only: from then on settings, resume points and play time were silently not saved,
  which looked like resume and BOOT JINGLE OFF breaking. The kernel never clears a dirty
  bit it found at mount, so the warning repeats until a repair. A clean reboot during a
  game was checked to leave the partition clean and resume. Should TETSU-DATA still go
  read-only, the menus say so (CARD ERROR): `storage::data_read_only` reads
  `/proc/mounts` with the battery each second, and before a game resumes at start. A
  restarts, which repairs it; B goes on, and the warning comes back after each game.
  Checked on the device with `mount -o remount,ro /mnt/data`: shown within a second, B
  back to the menus; A (a plain reboot) and the German wording weren't tried there
- A second card in TF2 (2026-11-11) adds games: those in its `roms` folder join the lists
  with TETSU-DATA's, unmarked, and a game at the same path on both cards shows once, from
  TETSU-DATA (`library::find_all_roms`). Their saves, states, covers, play time and
  settings stay on TETSU-DATA, as they are named after the game, so taking the card out
  loses nothing and a game moved between the cards keeps them. The card can be FAT32 or
  exFAT (`CONFIG_EXFAT_FS`), as most cards over 32 GB come; a Tetsu OS card's TETSU-DATA
  is used if it has one, or else its first FAT32 or exFAT file system, or the whole card
  without a partition table. The kernel side is patch 0018: `mmc2`, card detect on PE22
  and a 3.3 V supply on PE4 that stays on, from ROCKNIX, plus `mmc` aliases that keep
  TF1 at `mmcblk0` and TF2 at `mmcblk2`.
  `/usr/sbin/cards` knows a slot by its controller (`4020000.mmc`, `4022000.mmc`) in the
  card's sysfs path, not its number. `S15data` finds TETSU-DATA on TF1 only, so a second
  Tetsu card is never taken for it, and runs `cards mount`, which mounts TF2's games at
  `/mnt/tf2` with `umask=022`, waiting up to 2 s for the partitions. The launcher checks
  the slot once a second in its menus' loop, by the card's CID in
  `/sys/block/mmcblk*/device/cid`, which also tells a card swapped during sleep: a card
  going in is mounted, one coming out is unmounted (lazily if a file was open), and the
  lists are read again, with a sign at the top for 3 s: `TF2: 12 GAMES`,
  `TF2: CAN'T READ THE CARD` or `TF2: CARD TAKEN OUT`. A card that wouldn't mount is
  tried again only once taken out. The check also runs once before the games are first
  read, for a card the kernel found after `S15data`. Taking the card out during a game
  isn't guarded against. System info's GAME STORAGE shows both cards' free space while
  one is in, without the totals, for room.
  USB STORAGE gives the card in TF2 to the PC as a second drive, the whole card, so the
  PC can also format it: `lun.1` of the same mass storage function, so no endpoint more.
  With no card there, the PC sees an empty card reader. The screen leaves once both are
  ejected and says EJECT THE DRIVES; going back runs `partprobe` on the card, in case
  the PC partitioned it anew, `cards check` (`fsck.fat -a` or exfatprogs' `fsck.exfat -y`)
  and `cards mount`.
  WI-FI TRANSFER lists both cards' games; a game on TF2 is named `TF2:` and its path,
  which no FAT32 or exFAT file name can start with. With a card in TF2, the page shows
  both cards' free space and a SEND TO choice (TF1 or TF2, TF2 by default, kept in the
  browser), and uploads send `card=tf2`; a card gone since is refused as `no-card`.
  Checked on the PC: `cargo test` (the merge, the slot by controller and CID, the mount
  lookup), clippy, the device tree compiled with `dtc`, the patch with
  `apply-patches.sh`, and previews of the USB STORAGE screen and the sign in both
  languages
- Charging screen (2026-11-11): plugging in USB power starts the device, and when that
  alone started it, a battery with the charge level shows instead of the menus. The
  AXP717 keeps what turned it on in REG 0x20 (PWRON status, AXP717 datasheet V1.2,
  6.15.2.19): bit 0 the power button, bit 2 VBUS insert, bit 1 the IRQ pin, bit 3 a
  charged battery, bit 4 a battery insert; a software reboot clears it. REG 0x21 keeps
  what turned it off (bit 1 software, as `poweroff` does). Measured on the device: `04`
  after plugging in, `01` after the power button, `00` after a reboot. Whether VBUS
  starts the PMIC is set in its efuse, so it can't be turned off. `S05charging` mounts
  debugfs, reads the register from `/sys/kernel/debug/regmap/0-0034/registers` (it isn't
  volatile, and nothing reads it before, so the first read reaches the chip) and writes
  `/run/charging` when bit 2 is set without bit 0. Then `S21jingle` stays quiet, and the
  launcher (`charging.rs`), after setting the charge limit and before anything else,
  shows `screens::charging_screen` until the file is gone: the battery outline with an
  accent fill to the level, the percentage, CHARGING or CHARGED (status other than
  `Charging`, as at CHARGE TO's limit) and PRESS POWER TO START. The backlight turns off
  after 10 s (`bl_power`); any button or opening the lid lights it for another 10 s.
  The power button's script (`/etc/acpi/PWRF/00000080`) removes the file instead of
  powering off, also during the boot logo before the launcher runs, and the launcher,
  checking every 100 ms, goes on to the menus as after a power-button start (the setup,
  resuming a game), without the jingle. If `axp20x-usb/online` reads 0 for 2 s, it shows
  POWERING OFF and powers off. The rest of the boot (Wi-Fi, SSH, USB networking) runs as
  usual; the launcher idles at 0% CPU on the screen. `launcher --preview-charging
  FILE.ppm` renders it. Claude checked on the device with a hand-made `/run/charging`
  that it draws, darkens after 10 s, lights again on B, and that the power button's
  script starts the menus. The user confirmed on 2026-11-11 a real start from the cable
  with no jingle, the battery screen, the screen going dark and lighting on a button,
  the power button starting the menus, and that unplugging powered it off and it stayed
  off. A power button press during the boot logo of a cable start went straight to the
  menus (the log shows the screen skipped). Powering off with the cable in stays off,
  as VBUS starts the PMIC only when it rises
- The mini-HDMI port (2026-11-11) mirrors the screen at 640x480: a TV or monitor shows
  the same picture as the panel, menus, boot logo and games alike, and scales it itself.
  Patch 0001 already had the HDMI controller (`sun8i-dw-hdmi`, DesignWare HDMI 2.0 TX),
  its H616 PHY, TCON TV0 and the connector, and the launcher draws only into `/dev/fb0`,
  so the kernel's fbdev emulation does the mirroring: on a hotplug,
  `drm_fb_helper_hotplug_event` probes the connector with the 640x480 buffer as the size
  limit and drives HDMI from `crtc-1` (mixer 1, TCON TV0) with the same framebuffer the
  panel's `crtc-0` scans. The only change is `video=HDMI-A-1:640x480@60` in
  `extlinux.conf`: with a TV in at boot, the helper otherwise sizes the buffer for the
  largest mode, such as 1920x1080, and the launcher would fill its top-left corner. The
  mode is CEA 640x480p59.94 (25.175 MHz), which every HDMI sink must take; if the EDID
  lacks it, the kernel adds it. `FBIO_WAITFORVSYNC` waits on CRTC 0, the panel, so games
  keep the panel's timing, and the TV, at the same nominal rate but its own clock, may
  tear. Sound stays on the speaker, headphones or Bluetooth: HDMI audio needs the H616's
  audio hub (AHUB), which mainline doesn't have. Closing the lid still sleeps, which
  turns HDMI off with the display driver. Turning the panel off (sleep, USB STORAGE,
  WI-FI TRANSFER) is only its backlight, so the TV keeps showing those screens.
  A risk to check with a real TV: the HDMI `tmds` clock comes from `pll-video2` at
  297 MHz, so for 640x480 it is 24.75 MHz (297 / 12), 1.7% under the 25.17 MHz TCON TV0
  gives from `pll-video1`. Modes at 74.25 and 148.5 MHz divide exactly; this one doesn't.
  If the TV shows nothing or a broken picture, look there first.
  Checked by Claude over SSH, without a TV: `echo on > /sys/class/drm/card1-HDMI-A-1/status`
  forced a hotplug while the launcher ran, and the DRM state (`/sys/kernel/debug/dri/1/state`)
  showed `crtc-1` at 640x480 with `plane-4` on the panel's framebuffer, and no kernel errors;
  `detect` put it back. A boot with `video=HDMI-A-1:640x480@60e` (forced on) kept `fb0` at
  640x480 with both CRTCs on it, and the launcher drew at 3.95 s, as before
- WI-FI TRANSFER, in Settings > DATA and as SEND GAMES OVER WI-FI in the setup's games
  step (2026-11-10): a web page for a phone or PC browser on the same network, for
  getting games on without a cable or card reader. Off unless its screen is open:
  `transfer::wifi_transfer` starts `transfer_server::Server` (the `tiny_http` crate) on
  port 80 of every address, so also over USB at `10.35.35.1`, and stopping it stops
  uploads under way and waits for each request's thread. The screen shows the address,
  a six-digit code that is new each time (`/dev/urandom`, bytes from 250 up skipped), and
  a QR code of `http://<address>/?code=<code>` (the `qrcode` crate, version 3 at 5 pixels a
  module, dark on a white square with the 4-module border scanners need), then what is
  arriving with its percentage and what came of the last request. Not connected, it says
  so and checks again every 2 s.
  - Access: the code, from the QR address or the page's form, gets a random 128-bit
    session cookie (`HttpOnly`, `SameSite=Lax`) and a redirect that drops the code from
    the address; sessions end with the screen. Wrong codes are answered one at a time,
    each after a second, so guessing would take days on average. Every change also needs
    an `X-Tetsu` header, which a page on another site can't send without a preflight the
    server never answers. Plain HTTP: someone watching the network could read the cookie
  - The page (`transfer_page.html`, built in with `include_str!`) follows the device's
    language (English or German, its own table) and builds everything with DOM calls, so
    no file name becomes HTML. Games upload one at a time with progress: a game goes to
    its console by extension (`.bin`, `.img`, `.iso`, `.wav`, `.sub` and `.sbi` with the
    PlayStation, for a `.cue`), into the top-level folder in `roms` with most of that
    console's games, or a new one named after its key (`GBA`). A ZIP waits in `roms` until
    the game inside names its console. Uploads go to a hidden `.upload-<name>` beside the
    destination, are synced and renamed into place, and leftovers are removed when the
    server starts. The card keeps 16 MiB free, and FAT32's 4 GiB is the largest file.
    Per game: COVER takes any image the browser opens, shrunk with `fit_size`'s rule to at
    most 200 by 300 and sent as the PPM the launcher reads; covers show on the page as BMP.
    SAVE replaces the game's save at `save_path` (at most 64 MiB) and removes its
    `.resume`, which would load the old save over it; the page asks first if there is one.
    DELETE GAME runs `delete_game`, as Y does. Buttons that send say so with ⤒ and change
    with what is there (ADD COVER or REPLACE COVER, UPLOAD SAVE or REPLACE SAVE); downloads
    are links with ⤓ and the file's name. The header and the browser's tab show the logo,
    which `/logo.svg` draws from `icons::LOGO`, a square per pixel. Saves and screenshots download (only single
    names in `saves` and `screenshots`), and a search filters the games. Leaving reads
    the games again if anything changed
  - B leaves, and asks again while an upload runs; that upload is then refused as
    incomplete and its file removed. The lid only turns the screen off, so uploads go on,
    as in USB STORAGE; an empty battery leaves and powers off
  - Claude checked on the device on 2026-11-10, from the PC over the device's Wi-Fi: the
    QR code decoded from a screenshot, the login and wrong codes (1.0 s), refusals
    (no cookie, no header, a `.txt`, a path as a name, a 400x400 cover, a PNG as a cover,
    `..` and other folders for downloads), a `.gb` into `GB`, a ZIP's `.gba` into `GBA`,
    replacing, a cover sent and read back pixel for pixel, a save downloaded unchanged
    and removing the `.resume`, deleting with save and cover, 100 MB at 6.5 MB/s with the
    same MD5 on the card, the screen during an upload, B twice stopping one, and port 80
    closed after leaving. In a headless Chrome, the page's own code: the QR address
    login, the wrong-code page, the queue with a refused file, a 600x900 JPEG cover shrunk
    to 200x300, search and deleting. Not checked: a real phone, the German page, a
    PlayStation `.cue` with its `.bin`, a file over 4 GiB, the lid during an upload
- USB stays connected through sleep, so SSH works after opening the lid (patch 0012).
  Before, the device dropped off the bus until the cable was re-plugged
- The in-game menu (`pause.rs`, drawn by `canvas::pause_screen`). Tapping Menu alone
  pauses, and Menu + Start still quits directly. Resume, 8 save state slots per game
  (`saves/<game>.state1` to `.state8`), volume, brightness and quit. A game with battery
  memory also saves `saves/<game>.resume` on leaving and loads it at the next start, so
  in-game saves made after a slot are never rolled back. Volume and brightness are saved
  in `/mnt/data/settings.txt` and restored by the launcher. Claude tested every item on
  the device over SSH with injected presses and screenshots: saving, loading, levels,
  quitting, resuming and Menu + Start. The user tried it with FireRed
  on 2026-11-08 and confirmed it feels right
- QUIT's START OVER (2026-11-11). Left and Right on QUIT choose KEEP PLACE (as before) or
  START OVER, which quits without the `.resume`: the save file is still written, then the
  `.resume` is removed (`save::forget_place`), so the next start begins from the game's own
  save, as a real console does after power-off. State slots stay. Each visit to the menu
  starts on KEEP PLACE, so A twice by accident still keeps the place. A game whose save is
  its whole state (`Persistence::FullState`, Tamagotchi) has no `.resume`, so its QUIT shows
  no choice. Menu + Start, lid sleep, power-off and SWITCH GAME keep the place. Tested on
  the PC (`pause.rs` tests and `--preview-pause` in both languages); the user checked it
  on the device on 2026-11-11
- A stale `.resume` is skipped (2026-11-12). Leaving writes the save file first and the
  `.resume` after, so a `.resume` older than the save file (`save::resume_is_stale`, by
  modification time) means the game ended without leaving: the reset button (hardware,
  unseen by Linux), a power cut or a crash, after the game saved in the meantime. A state
  holds the game's save memory, so loading it would bring the older save back and the
  2-second autosave would write that over the newer save file. The `.resume` is removed
  and the game starts from its save file instead. A save sent by USB STORAGE is covered
  too. Checked on the device: a `.resume` dated before the `.srm` was skipped and removed;
  a normal stop and start still resumed
- The in-game menu's lists (2026-11-11), as fifteen rows had filled the panel to its edges.
  The main list has RESUME, SAVE STATE, LOAD STATE, CHANGE DISC, SCREENSHOT, DISPLAY,
  OPTIONS, ACHIEVEMENTS and QUIT, at most 9 rows (`pause::MOST_ROWS`) of 32 lines instead
  of 22. DISPLAY opens a list with BRIGHTNESS, SCALING, SCREEN EFFECT, LCD GHOSTING and
  BORDER, and OPTIONS one with CONTROLS, CORE OPTIONS and CHEATS (`pause::List`); the title
  names the list. Rows that open a list or page show `>`. B goes back from a list to the
  row that opened it, and from the main list resumes; Menu resumes from any list. Opening
  the menu again starts on RESUME in the main list. A page that CONTROLS, CORE OPTIONS or
  CHEATS opens goes back to OPTIONS. The hint stays below where the longest list ends, so
  it doesn't move from list to list. VOLUME left the menu: the volume buttons do the same
  and show their sign over it. Checked with `cargo test` and `--preview-pause` in English
  and German; not yet on the device
- The game switcher (2026-11-11, `switcher.rs`): SWITCH GAME in the in-game menu's main
  list, just above QUIT, opens the RECENT games other than the one running, newest first
  (up to 7), leaving out any whose file is gone. It is left out of the menu when there is
  none, so the main list has at most 10 rows (`pause::MOST_ROWS`). A switches: the game
  saves as when quitting (resume state, play time, achievements), shows SWITCHING TO with
  the next game's console and name, writes that game to `playing.txt` and exits with code
  4 (`switcher::SWITCH_EXIT`). The launcher's `play_game` reads it back as
  `Ending::Switch`, and `act` starts that game straight away, which resumes from its own
  `.resume` and moves to the front of RECENT, so the game just left is first in its
  switcher and A, A goes back. A power-off between the two resumes the chosen game at
  boot, and a chosen game that can't start is forgotten there. After the last game quits,
  the list selects the game played last, if it is in that list. The menu passes the game's
  path as the lists have it (`GameSpec::listed`), which a ZIP's unpacked file isn't.
  Settings > IN GAME > GAME SWITCHER (`switcher=`, `settings::SwitcherStyle`) chooses how
  it looks: LIST (the default), rows like the menu's with the console on the right, Up
  and Down wrapping; or COVERS, the selected cover large in a frame of the theme's colour
  (fit to 200x230) with two smaller ones (88x116) each side, the name, console and place
  (`2 / 7`) below, Left and Right stopping at the ends. Covers are read and shrunk in the
  game's process when the page opens (`BoxArt`, so a screenshot stands in as in the
  lists, and a game without either shows its console's pictogram on a card). B or Menu
  goes back to SWITCH GAME. Menu + Select during play (`pause::SWITCHER_BUTTON`,
  `pause::Opening`) opens the page straight from the game, where B or Menu resumes and the
  hint says so; with no other game it does nothing. Like Menu + Start, the game doesn't
  see the Select: the page opens before the next frame, and the game continues only once
  every button is let go. `--preview-switcher` and `--preview-switcher-covers` render
  both. Claude checked on the device with injected presses and screenshots on 2026-11-11:
  both styles, Super Mario Bros. to Super Mario World and back several times, each
  continuing exactly where it was left (1-2 with the same score and coins), `playing.txt`
  and RECENT after each switch and quit, B on the page, and the selection after quitting.
  Menu + Select, injected in one write: it opened the covers page over Super Mario Bros.,
  B went straight back into the game, and it switched to Super Mario World; a Menu tap
  still opens the menu.
  One switched start of Super Mario World logged a single 451 ms underrun, a normal start
  and two more switches none. Not checked: the feel, the SWITCHING TO screen by eye (the
  switch is over before a screenshot over SSH), a PlayStation game, a ZIP, a game on TF2,
  hardcore, a power-off mid-switch, Menu + Select on the real buttons, the German wording
  on the device
- The volume sign: a press of a volume button shows `VOL`, a bar and the percentage,
  centred at the top of the screen for 1.5 s, in the menus, during a game and in the
  in-game menu (`screens::volume_sign`, `app::show_volume`). It is 20 pixels tall and 6
  from the top, so it stays in the border above a GBA picture, and covers the top of
  taller pictures. `fb::Framebuffer` holds it: `draw` stamps it on each screen, and `blit`
  writes around its area, so a game's picture never overwrites it and it doesn't flicker,
  with no cost per frame beyond splitting those rows. When its time is up, the menus draw
  again, and a game fills the area with its picture and black around it
  (`libretro::picture_in_area`), as it only redraws its own area. During a game the
  launcher process still changes the volume, as `amixer` there doesn't hold up a frame,
  and writes the new percent as one byte to the game's standard input
  (`libretro::volume_levels`). So the game shows the level that was set, not one read too
  early. Only the buttons show it: VOLUME in the settings has its own
  bar. Claude checked it on the device with injected presses and screenshots on 2026-11-09:
  a game list, Wi-Fi, the title keyboard, Super Mario Land during play and in its menu, and
  closing the menu with the sign up. A press every 1.4 s or so during play kept 59.6-59.75
  fps with no underruns, the same as without. The user confirmed on 2026-11-09 that it
  feels good on real presses, and that the lowest step is quiet but audible
- Volume moves in steps of 5%, by the buttons and by the VOLUME row in Settings
  (`settings::VOLUME_STEP`). The hardware has levels 0 to 31 at 1.5 dB each, with 0 and 1
  muted, so each step uses its nearest level (`percent_level`: 0, 2, 3, 5, 6, 8 ... 29, 31),
  which makes only 0% silent. `settings.txt` still saves the level, and one between steps,
  from an older version, snaps to the nearest step at start (`level_percent`). Before, a
  press moved two levels, so going down from the top showed different numbers from going
  up. Checked on the device on 2026-11-09: a walk from 0 to 100 and back hit the same levels,
  stopped at each end, and a saved level of 1 came back as 2
- During a game two processes save `settings.txt`, and saving writes every setting, so
  each reads the file again just before: the launcher before a volume button's save, or it
  undoes the game menu's display changes, and the game before saving those, or it brings
  back the old volume. Reproduced and checked on the device on 2026-11-09
- `settings-backup.txt` (2026-11-12) holds the settings from before their last change.
  `settings::save` copies the file it replaces there first, if that file is whole (some
  lines, each `key=value`: `settings::looks_whole`). Loading uses the backup when
  `settings.txt` is missing, unreadable or not whole, and writes it back as
  `settings.txt`, also for S50dropbear, which reads `ssh=` itself. A reset right after a
  change once lost `settings.txt`'s FAT entry; the boot repair emptied it and every
  setting went back to its default. Now at most the last change is lost. Checked on the
  device: an emptied `settings.txt` loaded the backup (one volume step behind) and was
  written back
- NES through `package/libretro-fceumm`, SNES through `package/libretro-snes9x` (free for
  non-commercial use only), and Game Boy and Game Boy Color through mGBA. Consoles
  appear in release order, set by the order of `SYSTEMS` in `systems.rs`. New systems name saves with the full file name, such as `Tetris.gb.srm`;
  GBA and Tamagotchi keep `<name>.srm` and `tama.state`, so existing saves still load.
  Cores that make less sound per panel refresh than their sample rate, like NES and SNES
  at 60.10 fps, play it at a matching lower rate, or they would underrun every few
  seconds. The
  user confirmed all of their games play well. Headless, Yoshi's Island, Chrono Trigger
  and A Link to the Past emulate at 134 to 143 fps
- The launcher redesign. The home screen is a grid of tiles with pixel-art pictograms
  (`icons.rs`, drawn for this project): RECENT (the last 8 games, in
  `/mnt/data/recent.txt`), each console with games, and SETTINGS. A console opens its list
  of games, with short names from `names.rs`. Holding a direction repeats (`repeat.rs`),
  Left and Right page, B goes back, and Start opens settings anywhere.
  Screens and button handling are in `launcher.rs` and `screens.rs`. Claude checked every
  screen on the device with injected presses and screenshots, and the user confirmed on
  2026-11-08 that it feels very good
- Game Boy games fill the screen height. `CORE_OPTIONS` in `libretro/` turns off
  mGBA's Super Game Boy border, which shrank SGB-enhanced games to half the screen. Not
  yet seen by the user

- PlayStation through `package/libretro-pcsx-rearmed` (PCSX ReARMed, ari64 recompiler and
  NEON renderer, built with the makefile's `h5` platform, the same Cortex-A53). Games are
  `.cue` files with their `.bin` beside them in `roms` (also `.chd` and `.pbp`); the
  launcher lists only the `.cue`. No BIOS file is needed, because the core has its own
  replacement. The first memory card is the save memory, in `saves/<game>.cue.srm`; the
  second is off. The frontend skips reading a game into memory when a core sets
  `need_fullpath`, and for this core alone answers the option defaults it declares through
  `SET_VARIABLES` (`CoreSetup::declared_defaults`): unanswered, it kept no memory card and
  read nothing ahead from the disc, which caused 33 ms underruns while FF7 loaded. Pictures
  are shown at 4:3 (`CoreSetup::aspect`), since its modes run from 256 to 640 pixels wide.
  Frames of 128 lines or more are scaled in two bands on two cores (`Band` in
  `libretro/scale.rs`), the same output as one pass, which took FF7 from 8.7 to 4.1 ms per frame
  of the 20 ms that 50 Hz European games have. Claude ran Final Fantasy VII, Metal Slug X
  and Star Wars Episode I (all European) on the device: 50 fps, Metal Slug X in play at
  the lowest 480 MHz and 44 C with no underruns, FF7 sees a formatted card, and save
  states (4.4 MB) save and load. 50 Hz games set the panel to their own rate since
  2026-11-14 (below). The user hasn't tried it yet, so sound and feel are unchecked
- Sorting. Settings > THEME > SORT GAMES BY chooses NAME, RECENTLY PLAYED or MOST PLAYED
  (`settings::GameSort`, `sort=` in `settings.txt`, under THEME since GENERAL has no room).
  `library::sort_library` applies it to consoles' lists and favorites; RECENT stays in the
  order played and search results stay by name. Recently played goes by a game's newest save
  file and most played by its play time, as the details screen shows them. Games that tie,
  such as those never played, keep their order by name at the end. L1 and R1 only jump in
  a list by name (`Library::jumps_by_letter`), and the hint leaves out `L R ABC` otherwise,
  on RECENT as well. When the games are read again, the selection follows its game into
  the new order (`Launcher::selected_path`, `select_path`), in any sort and on RECENT.

  Claude tested on the device on 2026-11-09 with dummy games with chosen dates and times: each
  order was as expected in the Game Boy list, R1 did nothing and the hint changed outside
  NAME, and a game started from row 4 moved to the top under RECENTLY PLAYED with the
  selection on it. Sorting 1000 extra empty games took 0.41 s, which happens at each start
  and after each game, and not at all under NAME. `settings.txt` and `recent.txt` were put
  back, and the card's files were as before. FAVORITES sorted is covered by a unit test only

- Play time (`play_time.rs`). Each game keeps its time in whole seconds beside its saves, as
  `saves/<save file>.playtime`, so it follows the save name through being zipped or moved
  and is deleted with the saves. The game process counts the time between frames and leaves
  out any gap of a second or more, which covers the in-game menu, sleep and anything else
  that stops it without any of them telling it. Fast-forward counts as the time it took. It
  writes on opening the menu, on the lid closing, on leaving (with a power-off's signal too)
  and every 5 minutes, so a core crash costs at most that. Details show it as `UNDER A
  MINUTE`, `12 MIN` or `2 H 5 MIN`, adding what a grouped game's discs had as games of their
  own. A game with saves from before this says UNKNOWN until it is next played; the file
  isn't counted among SAVE FILES.

  Claude tested on the device on 2026-11-09 with a dummy copy of a game started from its list:
  20 s of play, 10 s frozen with `kill -STOP` as sleep would be, then 5 s read 25 at the menu,
  a 12 s menu and 10 s more read 36, and 8 s more ended by SIGTERM read 43. Details showed
  NOT YET, UNDER A MINUTE and, with a written 7500, 2 H 5 MIN, and deleting the game removed
  the file. The 5-minute write and a real lid closing haven't been seen on the device

- Play statistics (`play_log.rs`, Settings > STATISTICS). Each time the game process writes a
  game's play time, it adds the whole seconds gained to `/mnt/data/play-log.txt` under the
  local date and the console's key, one line per day and console (`2026-11-11 gb 1234`),
  merged and written atomically, so a year is a few kilobytes. A game's own `.playtime`
  has no dates, so the user chose to count only from the log, which starts empty: older
  play isn't in it, and the total can be less than the games' times added up. Deleting a
  game leaves its time in the log. A session over midnight goes to the day of each write,
  at most 5 minutes off.

  The page is the twelfth tile on the settings grid. It shows the total since the log's
  first day, the last 8 weeks (Monday to Sunday) as bars with their `H:MM` above and the
  Monday below, this week in the accent colour, and the consoles by time, five at a time,
  Up and Down scrolling. With no log it says NO PLAY TIME YET. The library reads the log,
  so it is current after each game. `--preview-statistics` renders it with made-up weeks.

- Display settings per console. Changing scaling, effect or ghosting in the in-game menu
  saves all three for that console, as `display.gb=fit,crt,smart` in `settings.txt`
  (`Settings::console_displays`, read with `display_for`). A console's key comes from
  `SYSTEMS`, so Game Boy and Color are separate. Settings > IN GAME is the default for
  consoles with no such line, with a note saying so and how many have their own, and USE FOR
  ALL CONSOLES, which takes A twice, removes those lines. RESET SETTINGS does too. A line
  with an unknown value is ignored as a whole. The game process finds its console from the
  game (`game::run_game`), through to `Host::console`. An existing setting carries on as the
  default, and a console takes it as its starting point the first time its menu changes.

  Claude tested on the device on 2026-11-09 with saves in `/tmp`: an effect changed in a Game
  Boy game wrote only `display.gb`, an NES game still showed the default, the Game Boy game
  showed its choice after a restart, the note counted one console, and a first press of USE
  FOR ALL CONSOLES changed nothing while a second removed the line. `settings.txt` was put
  back afterwards. How the effects look on each console is the user's to judge

- Borders around small screens (`libretro/border.rs`). Settings > IN GAME > BORDER and the
  in-game menu's BORDER choose what fills the screen around the picture: AUTO (default), OFF
  (black, as before), BEZEL or THEME. It is the fourth part of a console's display line, as in
  `display.gb=fit,none,smart,bezel`, and `border=` for the default; lines from before it have
  three parts and keep AUTO. AUTO gives a bezel to the handhelds (Game Boy, Color and
  Advance, Game Gear, Neo Geo Pocket and Color, WonderSwan and Color, Lynx), whose small
  screens leave the most around them, and black to the others; the in-game menu shows what
  it gives, as `AUTO (BEZEL)`. Tamagotchi is left out, as its picture hasn't been looked at.
  BEZEL draws a handheld's body in a colour that console came in (a plain dark grey for the
  others), lit from above, with rounded glass reaching 18 px past the picture, a light rim,
  a dark edge by the picture, a shadow on the body and a red power light where there is room
  left of the glass. Where the margin is too thin, the glass runs off the screen's edge so
  its corners don't show. No copied artwork or logos. THEME is the theme's background,
  darker downwards, with a 2 px line in the accent colour 4 px from the picture.

  The border is made for the whole screen once per picture size and choice
  (`Host::update_border`), so it costs nothing per frame; a size change, as the Mega Drive's
  modes make, draws it all again, which also clears what the old picture left. It is drawn
  at the start, after the menu (`play::draw_whole_screen`, in place of clearing to black),
  behind a sign whose time is up and where the fast-forward sign was (`picture_in_area`
  takes it), and the in-game menu dims it with the picture. The battery warning keeps its
  black box. LCD GHOSTING now shows only on cores with its option (mGBA), which makes room
  for BORDER: only PlayStation games have CHANGE DISC, so no game shows all 16 items.
  `--preview-borders` renders six of them as a sheet.

  Claude tested on the device on 2026-11-11 with a copy of Link's Awakening and its saves in
  `/tmp`: the bezel at FIT, 59.45 fps with 2.1 ms on the screen as before, the dimmed bezel
  behind the menu with AUTO (BEZEL), THEME chosen there showing at once on resuming and
  saved as `display.gb=fit,none,smart,theme`, and the fast-forward corner given back to the
  border after R2. `settings.txt` was put back. How it looks to the user, the other
  consoles, INTEGER, the German wording and a Mega Drive mode change are unchecked

- A game's details. Y in a game list opens them (`Screen::Details`, `screens::details_screen`):
  the file's full name, console, size, last played, how many save files, and the cover, with
  PLAY, STAR or REMOVE STAR (COLLECTIONS since 2026-11-13), and DELETE GAME AND SAVES. The screen holds the game's path, not
  its place in the list, as playing or starring can move it. `--preview-details` renders it.
  The size is what the game takes on the card, so a ZIP counts as zipped, and a `.cue` or
  `.m3u` counts all its tracks and discs (`game_files::GameFiles`). Last played is the date of
  its newest save file, since leaving a game writes one, so it needs no file of its own and
  knows games played before this screen; a game that never saved says NOT YET. It trusts the
  clock at the time, so a device that had no network time can show an odd date.

  Delete needs A twice in a row, and any other press calls it off; the screen always opens
  on PLAY. It removes the game first, so a failure there leaves the saves, then its saves
  (the memory card, resume point, 8 slots and any `.unreadable`), its cover, its star and
  its RECENT entry, and any folder that left empty, up to but not including `roms`. For a
  grouped or `.m3u` game that includes each disc, its tracks and the saves those discs had
  by themselves. Only what is under `roms` counts as the game's: a line in a `.cue` or `.m3u`
  that leads elsewhere is left alone. Anything another game on the card also uses stays,
  such as the save a cartridge shares with its ZIP or a cover of the same name, until the
  last game using it goes. There is no undo.

  Claude tested on the device on 2026-11-09 on dummy games, first listing the card's files:
  a cartridge with all its kinds of save and a cover; a cartridge and a ZIP sharing a save and
  cover, which stayed after the first and went with the second; a starred `.cue` with two
  tracks in its own folder; and a game grouped from two discs in a folder each, with its list
  and a card from before grouping. A first press followed by another button deleted nothing.
  Afterwards the card's files, `recent.txt` and `settings.txt` were as before. Failing to
  delete hasn't been seen on the device, and neither has a real game been deleted

- Jumping by letter. L1 and R1 in a game list go to the first game of the previous or next
  letter, and L1 inside a letter goes to that letter's start first. They wrap at the ends
  and repeat while held (`repeat.rs`). The letter is the first character of the name as
  shown, ignoring ASCII case, as lists sort; so a name with `The` in front counts as T.
  RECENT doesn't jump, being in the order played. Left and Right still page, though the
  hint no longer has room to say so, nor for B BACK. The hint calls this L R ABC.

- Search. X on the home screen or in a list opens the keyboard (`app::search_games`), showing
  how many games match while typing. OK shows them as a list like FAVORITES, with each
  console's icon, and does nothing at 0 matches. A game matches when all the typed words
  are in its shown name or file name, in any order, ignoring case and common accents
  (`library::find_games`). It looks through all consoles, also those with their tile hidden.
  `List::Search` reads `Library::search`, and `reload_library` runs the search again after
  a game or a star, so results stay. B leaves to where the search began, and X in the
  results edits the same text. They last until the launcher stops.

  Claude tested on the device on 2026-11-09 with injected presses and screenshots, without
  starting a game: "mario" counted and listed 7 games from four consoles, adding a letter
  counted 0 and OK stayed, B left, and starring in the results kept them. In the Game Boy
  list, R1 and L1 moved as described and holding R1 for a second kept jumping and wrapped.
  German was checked in previews. The buttons' feel on the device is the user's to judge

- ZIP games (`rom_zip.rs`, with the `zip` crate, deflate only). A `.zip` in `roms` lists as
  a game of the console its file inside belongs to, named after the ZIP, and an unknown,
  damaged or password-protected ZIP is left out with a line in the log. With several games
  in one, the first by name is used; other files and macOS's `__MACOSX` and `._` files are
  ignored, as are names with `..`. What a ZIP holds is read once per scan and cached, since
  `system_for` runs often. PlayStation games don't count, as a disc is several files, and
  a game over 128 MiB unpacked is refused, as `/tmp` is in memory.

  The game process unpacks to `/tmp/unpacked-game/<zip's name>.<game's extension>` before
  it opens the screen, gives that file to the core and removes it on leaving, so cores
  need nothing special. A power-off empties `/tmp`, and the next game replaces what a crash
  leaves. `save_path` names saves as for the unzipped game with the ZIP's name, so
  `Tetris.gb` zipped as `Tetris.zip` keeps its saves, states and cover, while RECENT and
  favorites keep paths and don't carry over. `scripts/boxart.py` doesn't know ZIPs; the
  on-device download does.

  Claude tested on the device on 2026-11-09 with ZIPs of 10 games, each also holding a
  `readme.txt` and a `__MACOSX` file. NES, Master System, PC Engine, Mega Drive, Game Boy,
  Color, Game Gear, SNES, Advance and Tamagotchi all started on their cores; a 16 MiB GBA
  game unpacked in 0.21 s. A PC Engine ZIP started from its list, saved a slot and quit,
  which removed the unpacked file, then resumed. 500 more ZIPs added about 0.06 s to the
  launcher's start, with the files already in the cache. Neo Geo Pocket, WonderSwan and
  Lynx use the same code but had no games to try, and the user hasn't played from a ZIP

- Multi-disc PlayStation games. An `.m3u` is a text file with one disc per line, such as
  `Final Fantasy VII (Europe) (Disc 1).cue`; names start from the `.m3u`'s folder, and
  lines starting with `#` are comments. `library::find_roms` reads every `.m3u` and leaves
  out the discs it names, ignoring case as FAT does, so the game shows once under the
  `.m3u`'s name. The core reads the file itself and holds up to 8 discs. All discs share
  one memory card and one set of states, in `saves/<game>.m3u.*`. A cover comes from the
  collection's first disc (`closest_cover`).

  Discs group by themselves without an `.m3u` (`library::group_discs`). Names that differ
  only by a disc number count as one game, across all folders, as some sets give each disc
  its own: `(Disc 2)`, `[Disk 2 of 3]` or `(CD2)` anywhere in brackets, or `Disc 2` or
  `CD2` at the end of the name. Tags must match, so `(Europe) (Disc 1)` and `(USA) (Disc 2)`
  stay two games. Of two copies of a disc, the first by path joins and the other stays its
  own game. The launcher writes the list to `roms/.discs/<game>.m3u` with full paths, only
  when it changed, which leaves the game folders as they are; a dot folder isn't scanned.
  If it can't be written, the discs stay separate. An old list of a game that no longer
  groups stays in `.discs` unused. Only PlayStation games group.

  Going from separate discs to one game changes the save name. So when an `.m3u` game has
  no memory card, `game::adopt_disc_save` copies the newest card of its discs, leaving
  theirs. States aren't copied, because each records its own disc number. A starred or
  recent disc no longer shows, because that path isn't a game any more.

  CHANGE DISC in the in-game menu (`Item::Disc`) appears when the core reports more than
  one disc, so multi-disc `.pbp` files should get it too; no such file has been tried.
  Left and Right choose a disc and A inserts it and resumes. The frontend answers the
  core's `SET_DISK_CONTROL_EXT_INTERFACE` (`DiskControl` in `libretro/host.rs`): it opens the
  lid, sets the disc and closes it, and PCSX ReARMed reports the lid as open for two more
  seconds, which is how a game notices. A disc whose file is missing is refused first,
  because the core would otherwise be left with no disc. If the core fails anyway, the
  previous disc goes back. The disc in use is part of a save state, so resuming and
  loading a slot bring back that state's disc, and the menu follows.

  Claude tested on the device on 2026-11-09 with generated discs of 300 sectors holding
  only a boot file and a looping program, as no PlayStation game was on the device. The
  list showed one game; the same disc and a missing third disc gave their messages;
  changing to disc 2 and back opened that disc's file (`/proc/<pid>/fd`); loading a slot
  and quitting and restarting both returned to disc 2; and starting from the list used
  `saves/Disc Test.m3u.*`. Grouping was tested the same way: two discs showed as one game,
  and the new card matched the newer disc's with both originals kept. With the user's
  real Final Fantasy VII (USA) discs 2 and 3, grouped from a folder each and run with a
  save in `/tmp`, changing disc during the intro opened disc 3 at about 60 fps with no
  underruns, and the intro carried on. A dummy disc's run logged one 15 s underrun after
  its change, which the real game didn't. No real game has asked for its next disc yet, so
  whether a game accepts the change at that prompt is unchecked
- Mega Drive, Master System and Game Gear through `package/libretro-genesis-plus-gx`, and
  PC Engine cards through `package/libretro-beetle-pce-fast`. Files are `.md`, `.gen` and
  `.smd`; `.sms`; `.gg`; and `.pce`. Mega Drive `.bin` isn't listed, because the launcher
  picks a system by extension alone and would show every PlayStation disc's `.bin` as a
  game, so rename those to `.md`. CD games aren't offered, as they need a BIOS and `.cue`
  is PlayStation's. The Mega Drive, Master System and PC Engine show at 4:3, since their
  pixels aren't square; the Game Gear keeps its 160x144 shape, 533x480 with side borders.
  Both cores answer their declared option defaults. Genesis Plus GX reports its save size
  differently once running: the full 64 KiB until the first frame, then only up to the
  last changed byte. So saves load fully, the first autosave rewrites the file at its
  trimmed size, and a game that has never saved gets a 0-byte `.srm`, which is harmless.
  Its licence bans selling it or using it commercially, like Snes9x's, which matters for
  a release. Claude tested with homebrew from `/tmp` on the device: Cave Story (Mega
  Drive) emulates at 206 fps headless and plays at 59.8 fps with 9.3 ms of work per frame
  at 1008 MHz, Astroswab (Master System) at 378 fps, and Jazz Jackrabbit (Game Gear) at
  503 fps. Save and load state and resume were proved on Jazz's level select, a Mega
  Drive save survived a reload, and states are about 1 MB. No PC Engine ROM was at hand,
  so a hand-written 8 KiB card checked that the core boots, draws correct colours, runs
  at 373 fps, keeps its 2 KiB save and writes an 80 KB state. The user tried all four
  consoles on 2026-11-09 and confirmed they work
- Neo Geo Pocket and Color through `package/libretro-beetle-ngp` (`.ngp`; `.ngc`, `.ngpc`),
  WonderSwan and Color through `package/libretro-beetle-wswan` (`.ws`; `.wsc`), and Atari
  Lynx through `package/libretro-handy` (`.lnx`, `.lyx`). Handy was chosen over Beetle Lynx
  because it has a built-in boot replacement, so no BIOS file is needed. Its `retro_init`
  reads a log callback from an uninitialised struct when the frontend has no log interface,
  and crashed on its first message, so `0001-initialise-log-callback.patch` starts it at
  NULL. All show in their own shape, with no `CoreSetup::aspect`. WonderSwan's native 75 Hz
  would play 20% slow when paced by the panel, but the core's default 60 Hz mode runs 5
  frames per 4 refreshes and reports 60.38 fps. WonderSwan has no Select button, so Select
  rotates the picture for vertical games: the core does it in software when `SET_ROTATION`
  fails, and has no automatic choice. Lynx rotates by the cartridge header, and Select
  overrides it. Two of these save outside the frontend's save memory: Neo Geo Pocket writes
  `saves/<game>.flash` when a game unloads, which every normal exit does, and resume states
  carry the flash too; Lynx writes `saves/<game>.eeprom`. WonderSwan uses the `.srm` as
  usual. Claude tested on the device from `/tmp` with homebrew: Asteroids Neo (Neo Geo
  Pocket) emulates at 154 fps headless and played at about 59.5 fps with 8 ms of emulation
  at 1008 MHz, a little under the panel's 59.94, so its feel is worth watching; Daybreak
  Ledger (WonderSwan) at 337 fps, playing at 59.8 fps; and the cc65 template (Lynx) played
  at 59.7 fps with 2.4 ms of emulation at 480 MHz. Save states and resume files were written
  on all three with no errors, restarting showed Neo Geo Pocket and Lynx resuming, and Select
  rotated WonderSwan. WonderSwan wasn't restarted to see it resume, and loading a state wasn't
  checked by its picture on any of the three. Only a WonderSwan ROM renamed `.wsc` tested
  that tile, and no game that actually saves to flash has been tried. Nobody has heard these
  systems' sound or tried their controls
- Doom and Cave Story (2026-11-12), the first games that aren't emulated: engines rebuilt
  as libretro cores, `package/libretro-prboom` (`.wad`) and `package/libretro-nxengine`.
  Both come with the system: NXEngine's source carries Cave Story's freeware data, and
  `package/freedoom` installs Freedoom: Phase 1 and Phase 2 (0.13.0, BSD, checked against the
  release's signed checksum). They go to `/usr/share/tetsu/games`, and `S15data` copies each
  folder to `roms` once, noting it in `bundled-games.txt` on TETSU-DATA, so a deleted game
  stays deleted and a folder of the same name is left alone; the first boot after the update
  took 3.5 s longer for the 60 MB. Since 2026-11-15 the copy runs in the background once the
  boot jingle has ended (`S15data` keeps the PID that `S21jingle` writes to `/run/jingle.pid`,
  which the launcher removes about 200 ms later, and waits 15 s at most), and a `.copying`
  folder left by a cut-off copy is removed first. Copying during the jingle kept the card
  writing, and the launcher's code, read from the card as it ran, took 1.5 s per animation
  frame: 2 frames instead of about 50, and the setup at 13.6 s. On the device, with the
  games and `bundled-games.txt` removed, twice: 51 and 52 frames at 3 ms, the launcher at
  6.65 s, both games copied 7.3 s after the kernel started. The games appear when the
  launcher reads the games again, as it does after the setup. `System::titles` names known files (`freedoom1` shows as
  "Freedoom: Phase 1", `doom2` as "Doom II", `Doukutsu.exe` as "Cave Story") in the lists,
  RECENT, SWITCH GAME, the details screen and the transfer page, with either GAME NAMES.
  Cave Story's `.exe` only counts when it is named `Doukutsu.exe` (`System::only_titled`),
  so DoConfig.exe and other programs aren't games, nothing can send one over Wi-Fi, and it
  can't be zipped, as it needs its `data` folder; the details screen counts and deletes that
  whole folder, unless the program sits loose in `roms`. Neither core saves a state the
  frontend can use (NXEngine has none; PrBoom's size changes as a level runs, which
  `load_state` refuses), so they use `Persistence::OwnFiles`: no save file, resume point,
  SAVE STATE or LOAD STATE, and nothing kept on leaving or at a lid close. Each saves from its
  own menu: Doom in `saves/<wad>/prbmsav*.dsg` (with `prboom.cfg`), Cave Story at its save
  points in `saves/profile.dat`, writing `settings.dat` in its folder. R2 doesn't fast-forward
  them, as Doom changes weapons with it. The frontend now answers `ENVIRONMENT_SHUTDOWN`, so
  Doom's own QUIT GAME goes back to the launcher. PrBoom calls its log callback unchecked in
  many places, and with no log interface every game crashed at address 0 in
  `M_LoadDefaults` (found with gdb on `qemu-aarch64-static`), so
  `0001-fall-back-to-a-log-function.patch` gives it one that prints warnings and errors;
  its search for optional MP3 music logs six `FindFileInDir: not found` lines per track.
  Its `prboom.wad` is built in. Measured on the device: PrBoom renders Freedoom at 192 fps
  headless and plays at 59.85 fps in 320x200, shown at 4:3 over the whole screen, with 7.8
  ms per frame at 1008 MHz; NXEngine at 441 fps headless and 59.9 fps at 480 MHz with 1.8 ms,
  its 320x240 doubled exactly. Claude checked the tiles and titles, a new Freedoom game saved
  from Doom's menu, quit, restarted and loaded at the same spot, Doom's own QUIT GAME, the
  in-game menu without the state rows, Cave Story's intro, and deleting Cave Story from its
  details screen removing its folder and play time. Nobody has heard either game, played
  them on the buttons, or reached a Cave Story save point
- Nine more consoles (2026-11-13). SG-1000 (`.sg`) runs on Genesis Plus GX, which already
  ships. Eight new cores, each its own package, built from a pinned commit:
  `libretro-stella` (Atari 2600, `.a26`; current Stella from `stella-emu/stella`, built in
  `src/os/libretro`, rather than Stella2014), `libretro-prosystem` (Atari 7800, `.a78`),
  `libretro-vecx` (Vectrex, `.vec`, built with `HAS_GPU=0`, as the frontend has no GL),
  `libretro-beetle-vb` (Virtual Boy, `.vb`, `.vboy`), `libretro-pokemini` (Pokemon Mini,
  `.min`), `libretro-potator` (Watara Supervision, `.sv`), `libretro-arduous` (Arduboy,
  `.hex`, a CMake package) and `libretro-picodrive` (32X, `.32x`). PicoDrive and Arduous
  have git submodules, so they download with `SITE_METHOD = git`. `.bin`, which several of
  these cores also take, stays unlisted, as for the Mega Drive. None of them needs a BIOS:
  vecx has the Vectrex's built in (an empty cartridge runs Mine Storm), PokeMini uses its
  FreeBIOS, ProSystem skips the 7800's, and PicoDrive emulates the 32X's own. All use
  `Persistence::BatteryRam` with declared defaults. Virtual Boy, 32X and Arduboy keep save
  memory in the `.srm`. PokeMini writes `saves/<game>.eep` itself when the game unloads, as
  the Lynx does. The 2600, 7800, Vectrex and Supervision have no save memory, so their games
  keep only the resume point and state slots. Only PicoDrive applies cheat codes, so only the
  32X has a `cheats` folder. Box art comes from the libretro-thumbnails folders of the same
  names, and RetroAchievements uses consoles 33 (SG-1000), 25, 51, 46, 28, 24, 63, 71 and 10
  (32X). The Supervision, Pokemon Mini and Arduboy are handhelds with a bezel. The tiles go
  in release order, from the Atari 2600 (1977) to the Arduboy (2016), after Cave Story.

  PicoDrive forces the libretro file interface (`USE_LIBRETRO_VFS := 1`), with which
  `dr_mp3.h` assigns stdio's `FILE *` to libretro's `RFILE *`, an error in GCC 15. The
  frontend doesn't offer that interface anyway, so it is built with `USE_LIBRETRO_VFS=0`.
  It builds for `platform=aarch64`, which uses the arm64 SH-2 dynarec for the 32X. PokeMini
  runs at the Pokemon Mini's 72 Hz unless `pokemini_60hz_mode` is on, which drops every
  sixth frame and is the core's default; it reaches the core because the frontend now takes
  the first options interface (see the cores' options below). `FRONTEND_DEFAULTS` sets
  `pokemini_video_scale` to 1x, so the frontend scales the 96x64 picture and draws its LCD
  effect and bezel, instead of the core's 4x dot matrix. Beetle VB shows the plain red and
  black picture by default (anaglyph mode with the preset disabled). Vectrex and Virtual Boy
  are 50 Hz, so they set the panel to 50 Hz, as PAL games do. PokeMini aborts on a file of
  0x2100 bytes or less, which it refuses, but a real cartridge is never that small, and a
  game runs in its own process.

  Tested by Claude on the device, with free homebrew and test ROMs: Warcraft 32X (black, as
  it needs the user's Warcraft demo data), the 32X 240p Test Suite (its menu, at 256 fps
  headless and 59.63 fps playing with 4.2 ms of emulation at 720 MHz), the Capitan Sevilla 3D
  demo for the Virtual Boy (124 fps headless, 50.41 fps playing, 6.6 ms at 1008 MHz),
  Catacombs of the Damned for the Arduboy (190 fps headless), Mine Storm on the Vectrex (166
  fps headless, 50.03 fps), colour bars written for the 2600 (157 fps headless, 59.84 fps after
  the first seconds), PokeMini's FreeBIOS padded to 16 KiB ("NO CART", 600 fps) and idle
  loops for the 7800, Supervision and SG-1000 (over 300 fps). The tiles and icons, the Virtual
  Boy game from the menus with SAVE STATE, CORE OPTIONS, QUIT and a resume at the same
  screen, and RetroAchievements identifying it, were checked through the screen. The Arduboy
  plays at about 57 fps, because schedutil keeps the CPU at 720 MHz while it needs 7.6 ms per
  frame there with uneven frames; at 1512 MHz it played at 59.84 fps. Nobody has heard these
  consoles, played them on the buttons, or tried a real 7800, Supervision, SG-1000 or Pokemon
  Mini game
- With more consoles a full home screen reached four rows, too short for a tile's text, so
  GRID SIZE's NORMAL now shows three rows at a time and pages beyond, as LARGE does

Built but not yet tested on the device:

- Bluetooth headphones (stage 2), through bluez-alsa 4.3.1 with SBC, which every headset
  has (AAC, aptX and the others need libraries that aren't in the image).
  `/usr/sbin/bluetooth` starts `bluealsa -p a2dp-source` after bluetoothd. bluealsa's
  `20-bluealsa.conf` makes the ALSA device `bluealsa`, a `plug` that converts the
  sample rate. `audio_out.rs` follows whether a headset is connected, from a thread
  that asks bluealsa (its ObjectManager at `/org/bluealsa`, an `A2DP-source` PCM in
  `sink` mode) once a second, and only while the system bus runs. Menu sounds and game
  audio then play with `aplay -D bluealsa`, the volume buttons set the PCM's `Volume`
  (0 to 127 per channel, which bluealsa sends to headphones that take it or applies
  itself), and the codec's level is kept for the speaker. A game checks every 60
  frames and starts aplay again on the other device when a headset connects or goes,
  or when aplay ended, as it does with `No such device` when the headset is turned
  off. The boot jingle stays on the speaker, since Bluetooth starts after it.

  BlueZ only had its audio plugins after a clean rebuild: turning
  `BR2_PACKAGE_BLUEZ5_UTILS_PLUGINS_AUDIO` on with `bluez5_utils-reconfigure` kept the
  old `src/builtin.h`, so a2dp and avrcp were compiled but never loaded, there was no
  `org.bluez.Media1`, and a headset's connection failed with
  `BREDR.ProfileUnavailable`. A BlueZ option change needs `bluez5_utils-dirclean`.

  Tested by the user on 2026-11-11 with a Bose QC35 II: pairing, playing a game's
  sound, and the sound moving to the speaker and back as the headphones went off and
  on. BlueZ names it "LE-Bose QC35 II", from its Low Energy advertising; the same
  device also shows as "Bose QC35 II" before pairing. Each move to the headphones
  starts with underruns of 0.5 to 1.1 s while the A2DP stream starts, and the game
  ran at 55 fps for those seconds, then 59.7. One reconnect failed once with
  `Couldn't change BlueALSA PCM configuration: Input/output error`, and the next check
  a second later started it. Not yet tried: the volume buttons, how the delay feels.

- Bluetooth, stage 1 of 2 (controllers; headphones are stage 2, above),
  from Settings > BLUETOOTH, off by default. The RTL8821CS's Bluetooth half is on UART1
  with the Realtek 3-wire (H5) protocol, already in the shared device tree
  (`realtek,rtl8821cs-bt`, enable PG19, device wake PG17, host wake PG16). The kernel
  has `CONFIG_BT`, `BT_HCIUART_3WIRE` and `BT_HCIUART_RTL`, `UHID` for BlueZ's input
  plugin, and the PlayStation, Sony, Nintendo and Microsoft HID drivers with their
  vibration. `rtl8821cs_fw.bin` and `rtl8821cs_config.bin` are built in like Wi-Fi's;
  linux-firmware's WHENCE makes the config a link to `rtl8761b_config.bin` (25 bytes),
  the one ROCKNIX uses on the H700, so `external.mk` installs those two files only.
  BlueZ 5.86 with D-Bus is in the image, but `post-build.sh` takes the S off their init
  scripts; `/usr/sbin/bluetooth on|off|status` starts and stops both, and unblocks or
  blocks the radio through rfkill. The launcher runs it after its first frame when the
  setting is on, and from the screen's ON/OFF row. It costs about 5 MB of RAM
  (bluetoothd 3.9 MB, dbus-daemon 1.7 MB).

  The screen (`bluetooth.rs` talks to BlueZ through `zbus`, `bluetooth_menu.rs` has the
  list and buttons, `bluetooth_screen.rs` runs it) searches while open and lists
  connected devices, then paired ones, then the rest by signal, leaving out those that
  never said their name. A pairs a new device, trusts it so it can reconnect by itself,
  and connects it; on a paired device A connects or disconnects, and X twice forgets it.
  BlueZ needs an agent even for Just Works pairing, and without one refused the Backbone
  Pro with `AuthenticationFailed` (`No agent available for request type 2` in
  `bluetoothd -d`). While the screen is open, the launcher is the default agent
  (`bluetooth::Agent`, NoInputNoOutput): it accepts every confirmation and answers a PIN
  request with 0000, since there is no screen for a passkey.

  No BlueZ call runs on the screen's thread. Starting a search while a classic pairing
  (a Samsung TV, chosen by mistake) kept the radio busy waited 15 s for BlueZ's answer,
  and the screen froze that long, then carried out the presses made meanwhile. A reader
  thread now connects, registers the agent, searches and sends the devices every second;
  pairing, connecting, disconnecting and forgetting each run in a thread, one at a time. Pairings are
  in `/var/lib/bluetooth` on the root filesystem: they survive reboots but not a flash,
  because TETSU-DATA is FAT and BlueZ's folder names have colons. Most Bluetooth
  controllers report the D-pad as a hat, so `input::Axes` turns `ABS_HAT0X/Y` and the
  left stick (on past half way, off under a third) into D-pad presses, for gamepads only
  (a south button and no accelerometer, as a DualShock 4's motion sensors are a device of
  their own). A controller then acts as the device's own buttons, in menus and games.

  Measured on 2026-11-10:
  - Boot: the Realtek driver held the chip in reset for 100 ms and waited 500 ms in probe,
    which the kernel waits for before mounting the root filesystem, so init started at
    0.91 s instead of 0.53 s and the launcher was drawn 0.35 s later (6.70 s against
    6.36 s with the driver kept out by `initcall_blacklist=hci_uart_init`). Patch 0017
    moves the wait to a delayed work. The firmware upload (0.75 s over the UART) then
    still overlapped the first init scripts, 0.2 s, so
    `hci_uart.rtl_boot_sync_delay_ms=5000` holds back the first H5 link sync at boot: it
    now loads 6.1 s after the kernel starts. Six boots drew the launcher at 6.33 to 6.46 s,
    one at 6.76 s, unexplained. Delaying the reset itself instead broke Wi-Fi on every
    boot: rtw88 timed out on SDIO (-110) loading its firmware at 0.84 s while the
    Bluetooth half was held in reset.
  - Wi-Fi with Bluetooth on: with rtw88's deep power save, the Wi-Fi firmware logged
    `firmware failed to leave lps state` and once stopped sending, connected, until
    Wi-Fi reconnected; Bluetooth off alone didn't bring it back. With
    `rtw88_core.disable_lps_deep` set, 170 s of searching lost no ping (85 of 85), and 50
    such errors came back when it was cleared again. Since 2026-11-13 it is set for good
    on the kernel command line (`rtw88_core.disable_lps_deep=Y` in `extlinux.conf`), see
    "Wi-Fi hang with Bluetooth off" below.
  - Wi-Fi hang with Bluetooth off (2026-11-09 and 2026-11-13): connected, with an
    address, but nothing sent (no ping to the gateway, no DNS, no DHCP answer), while
    receiving still worked; rtw88 logged `failed to get tx report from firmware` from
    312 s after boot, at first once a minute, then several a second, and a WARNING ran
    `rtw_watch_dog_work` > `rtw_enter_lps` > `rtw_sdio_deep_ps` >
    `rtw_power_mode_change`. Restarting `S42wifi` didn't help, a reboot did. So deep
    power save hangs the firmware with Bluetooth off too. The driver reads
    `disable_lps_deep` only in `rtw_core_start`, when the interface comes up, so the
    runtime switch `/usr/sbin/bluetooth` used to make took effect only at the next Wi-Fi
    restart, and turning Bluetooth off switched deep power save back on. Now it is set
    from the start and the script leaves it alone. Plain power save (POWER SAVING,
    `iw ... power_save`) stays on. Not yet measured: what it costs in battery, and
    whether the hang stays away over many hours.
  - Sleep: the Realtek driver puts the Bluetooth half in reset while asleep and probes it
    again on waking. With an RTC-timed s2idle (Wi-Fi disconnected first, as the launcher
    does), Wi-Fi reconnected and pinged after waking, with Bluetooth off and on, and the
    adapter was powered again after it.
  - On the device, Claude saw the BLUETOOTH tile, the screen turning on and off, its
    search listing four named devices nearby, and D-Bus, bluetoothd and the rfkill block
    following the setting. On 2026-11-11 the user paired a Backbone Pro (LE only, HID over
    GATT, driven by `hid-microsoft` as `045E:0B13`) and its buttons worked in the menus,
    until the freeze above. Its first pairing then failed every reconnect: the Backbone
    reconnected by itself every 2 s, the kernel started encryption with the stored Secure
    Connections key, the Backbone never answered, and the link hit its 1 s supervision
    timeout (`btmon`). Not Wi-Fi (the same with it disconnected) and not the stored
    connection parameters (the Backbone asks for the same, 15 ms, latency 4, 1 s).
    Forgotten and paired again (one attempt got no SMP Pairing Response in 30 s, the next
    worked), it failed its reconnects the same way, so the pairing wasn't the cause; nor
    were the connection parameters (the kernel's defaults, latency 0, failed too). The
    traces showed the difference: while pairing, encryption started about 1 s in, after
    the Backbone's connection parameter update and the data length change; on a
    reconnect BlueZ started it 26 ms in, and the Backbone went on sending unencrypted
    L2CAP data as if the request never came. The BlueZ patch
    `bluez5_utils/0001-device-raise-le-security-on-demand.patch` leaves the level low on
    connect (`device_attach_att()`, and `device_connect_le()` for Device1.Connect) and
    lets the first read the Backbone refuses with Insufficient Encryption raise it,
    which happens 0.45 to 0.7 s in. With it the Backbone reconnected, encrypted and
    worked, also after being turned off and on (user, 2026-11-11). The Backbone takes a
    new static address with each pairing. `btmon` is in the image
    (`BR2_PACKAGE_BLUEZ5_UTILS_MONITOR`) for traces like these.
  - Its buttons then did nothing after a reconnect: it came back as `event6`, the name
    it had before, and `input::spawn_readers` never opened a name twice. It now opens a
    name again once the reader of the device that had it has ended.

  The search retries until the adapter is powered: at first it failed with `NotReady`
  and never ran, while the screen said SEARCHING.

  `push.sh rootfs` no longer copies `/etc/passwd`, `/etc/group` and `/etc/shadow`:
  Buildroot adds package users such as `dbus` only to the image, so the build tree's
  copies lack them, and pushing them removed the user D-Bus runs as. The unit was given
  the `dbus` user and the `/var/lib/dbus` link by hand, as in the image.

- Faster boot with a logo. The launcher logs when it is drawn after the kernel started
  and after power on, from the system counter (`since_power_on` in `system_info.rs`), which
  gives the time before the kernel exactly. U-Boot's `CONFIG_BOOTDELAY` is 0 instead of
  2, and its diagnostic writes of `bootcount.txt` and `uboot-env.txt` are gone, which
  cost 0.30 s. The launcher now draws 3.61 s after power on, 2.53 of them before the
  kernel. U-Boot reads the card at about 24 MB/s, measured with a kernel padded by
  10 MiB, so the 21 MB kernel takes 0.87 s to load. An LZ4 kernel (10.6 MB, in the
  frame format `booti` unpacks, made with Buildroot's `host-lz4`) was tried on
  2026-11-08 and was 0.11 s slower over three boots each (3.73 s against 3.62 s), so
  unpacking costs about 0.55 s; the plain `Image` stays. Kernel messages go to
  `ttynull` (`CONFIG_NULL_TTY`) at `loglevel=5`: the boot logo needs a level above
  `CONFIG_CONSOLE_LOGLEVEL_QUIET` (4), and at 3 it didn't show. With `console=tty3`,
  warnings such as `4021000.mmc: card claims to support voltages below defined range`
  still printed over the logo, as the user saw, because tty3 isn't allocated that
  early and the kernel falls back to the visible tty1. The
  logo is `board/h700/logo.ppm`, a plain PPM made with `launcher --preview-splash`,
  copied over the kernel's by a hook in `external.mk`, and shown centred from about
  0.44 s after the kernel starts. The init scripts take 0.01 to 0.04 s each.
  `scripts/push.sh uboot` writes U-Boot at 8 KiB on the card over SSH and checks it by
  reading it back. The user saw the logo with a kernel warning over it; with `ttynull`
  it is clean. On 2026-11-08 the logo became TETSU over a smaller OS, and the user saw it
  on the device with no kernel text over it. On 2026-11-09 the user chose the project's
  logo, a tetsubin (iron kettle, as tetsu means iron) with a gamepad's controls on its
  side: `icons::LOGO`, on the launcher's 32 by 24 grid, drawn above TETSU OS at 5 times
  its size. Its handle, lid and body mirror around x = 17, and the spout balances the
  body sitting a pixel right of the grid's centre

- Scrolling names: a selected name too long for its row pauses 1.2 s, scrolls at 48
  pixels a second to its end, pauses, and starts over (`screens::scroll_offset`). Only
  that row is redrawn, 20 times a second (`fb::draw_area`), which kept the launcher at 2%
  CPU. The battery is now read once a second rather than on every redraw
- Favorites: Select stars or unstars the selected game in any list, kept in
  `/mnt/data/favorites.txt` (`favorites.rs`). Starred games show a yellow `*` before
  their name, and a FAVORITES tile with a star icon follows RECENT on the home screen,
  listing them by name with their console's icon. A list that empties, as favorites do
  when the last is unstarred, goes home at once. Claude tested starring, the tile, the
  list and unstarring on the device
- **Collections and hidden games** (2026-11-13): the user's own lists beside FAVORITES,
  and games kept out of every list.
  - A game's details (Y) have a COLLECTIONS row, which replaced STAR (the screen has room
    for six rows), with how many of the favorites and collections the game is in. It
    opens a checklist (`Screen::GameCollections`, `library::collections_menu`):
    FAVORITES, each collection, then NEW COLLECTION, with ADDED on those the game is in.
    A puts it in or takes it out; NEW COLLECTION types a name on the keyboard and puts the
    game in it; Y on a collection renames it. Select still stars a game anywhere.
  - Kept in `/mnt/data/collections.txt` (`library::collections`): each name in brackets on
    its own line, then its games' paths in the order added. A collection goes once its last
    game leaves it, so none is ever empty in the file. Names are at most 11 characters,
    which fit a NORMAL tile (longer labels are cut with `..`), without `[`, `]` or `,`
    (the settings' hidden tiles are a comma list), and two can't differ only in case, as
    the screen shows capitals. A name already taken isn't accepted, and the keyboard's line
    says so.
  - Each collection with games has a tile after FAVORITES (`Tile::Collection`, the stacked
    boxes of `icons::COLLECTION`), with its count, and its games joined on the home
    screen's header line. Its list is like FAVORITES: by name or the chosen sort, with each
    console's icon, letter jumps when by name. One whose games are all on a TF2 card that is
    out keeps no tile but stays in the checklist and the settings list. HOME SCREEN TILES
    lists every collection, saved as `collection:<name>` in `hidden_tiles=`; a rename
    carries that along and a delete removes it.
  - Settings > DATA > COLLECTIONS lists them with their game counts: A renames, X twice
    deletes (any other press calls it off). Games stay on the card. They are in DATA, not
    GENERAL, as GENERAL has 8 rows and the settings screen fits 9 with no note.
  - HIDE GAME on the details hides a game (`/mnt/data/hidden-games.txt`, one path per
    line, `library::hidden`) and goes back to the list. A hidden game leaves its console's
    shelf before every other list is picked from it, so it is out of the console lists,
    RECENT, FAVORITES, collections, search, the tiles' counts and SWITCH GAME
    (`switcher::others`); a console with only hidden games has no tile. Names are made
    before hiding, so the others' short names don't change. Its files, saves and play time
    stay; it still counts when deleting another game's shared files, a power cut during it
    still resumes it, and the transfer page still lists and deletes it. Settings > DATA >
    HIDDEN GAMES lists them with their console's icon, and A shows one again.
  - Deleting a game takes it out of the collections and the hidden list too. A backup
    includes `collections.txt` and `hidden-games.txt`.
  - Previews: `--preview-collection-tiles`, `--preview-collection`,
    `--preview-collections-details`, `--preview-game-collections`, `--preview-collections`
    and `--preview-hidden`, with sample collections and hidden games
  - Claude checked on the device: a collection made from the details with the keyboard,
    a second game added, its tile and list, a game hidden from that list and shown again
    in HIDDEN GAMES, a rename and a delete with X twice; the files were removed after.
    Unit tests cover the file, the lists built from it, the presses and the screens

- Box art. `scripts/boxart.py` lists the games on the device over SSH, fetches covers
  from the libretro-thumbnails collections on GitHub (following their symlink stubs, and
  falling back to the closest name, such as one with `(GB Compatible)`), shrinks them to
  at most 200x300 with Pillow, and copies them as PPM files to `/mnt/data/boxart`.
  Downloads are cached in `~/.cache/rg35xxsp-boxart`, never in the project, because the
  covers are copyrighted. All 35 games got a cover. Game lists show the selected game's
  cover in a panel on the right (`boxart.rs`, `screens::list_screen`), or the console's
  icon without one, which leaves about 31 characters for names. Claude checked the GBA
  and SNES lists in screenshots, and the user said the covers look fine
- Network time. BusyBox `ntpd` (`S43ntpd`, from `pool.ntp.org`) runs from boot and
  keeps retrying, so it syncs whenever Wi-Fi is up, and steps the clock on its first
  answer. `/usr/libexec/ntp-synced` writes the RTC with `hwclock -w -u` on each sync and
  logs it to `/var/log/ntp.log`; the kernel reads the RTC at boot (`CONFIG_RTC_HCTOSYS`).
  The clock had been 32.6 hours fast. Claude checked on the device: the first sync
  stepped it by -117520 s to match the PC, and after a reboot it was right from the RTC
  (0.7 s off) and synced again 16 s into boot, though ntpd started before Wi-Fi. The
  system clock stays UTC
- Theme settings, a THEME tile in Settings: COLOR (9 accent colours, all bright enough
  for the dark text on a selected row), TITLE (SHOWN or HIDDEN) and TITLE TEXT, typed on
  the on-screen keyboard (`screens::KeyboardView`, shared with Wi-Fi passwords), up to
  24 characters with case kept; saving it empty brings back TETSU OS. The accent
  is part of the `Theme` each canvas carries, so a game's menu uses it too. Saved as `accent=`, `title_hidden=` and
  `title=`. Claude checked the colour on the menus and a game's menu (from
  `--preview-pause` on the device), editing, hiding and resetting the title; the test
  unit is back on the defaults. The user then chose blue with the title hidden. Without
  the title, the home grid starts at 36 instead of 84 pixels (`GRID_TOP_UNTITLED`, just
  below the clock), so 3 rows of tiles grow from 108 to 124 pixels, and each tile's icon
  and labels are centred in it
- The home screen's header (2026-11-11, chosen by the user from PC mock-ups): a band 74
  pixels high, 10% lighter than the background, with a 2-pixel accent line under it
  (`screens::home_header`). On it are the boot logo's kettle (`icons::LOGO`) at twice
  its size, the title right of it, and the clock and battery top right. The default
  title shows as on the boot screen, TETSU in the accent and a smaller grey OS. A typed
  title is in the accent at the same size, or smaller when it would reach the clock.
  TITLE HIDDEN removes the whole header, as before. Under the title, the selected tile's
  name in white, then a grey line about it: RECENT the last game and when, FAVORITES
  their names, a console `LAST: <game>` and when, or NOT PLAYED YET, and SETTINGS what
  is in it. The line is cut with dots to fit, but never its time. The last played game
  of a console is the first of its games in the recent list, which was played after any
  other, or else the one with the newest save (`library::last_played_game`). It is
  kept in the library, which is read again after a game, so the home screen reads no
  files after the first look at each tile. `clock::ago_text` gives JUST NOW, minutes,
  hours (also up to 6 hours back past midnight), YESTERDAY, days, and after a week the
  date, by local days. Tests cover the band, the logo, the title sizes, cutting, the
  last played game and the times. Checked only in PC previews, in English and German
- Menu sounds: Settings > GENERAL > MENU SOUNDS, ON or OFF (default). Directions and
  volume keys tick, A, Start and Select make a rising blip, B a falling one, in the
  launcher's own screens (`menu_sound` in `app.rs`) and on the on-screen keyboard, for
  Wi-Fi passwords and the title, where X and Y blip too (`keyboard_sound`). `sounds.rs` makes them as
  tones, 14 to 52 ms at 48 kHz, quiet next to games, and plays each through its own
  aplay, which holds the card about 70 ms; a sound is skipped while the last still plays,
  and `play_game` waits for it (`sounds::wait`), since the card takes one stream. Groups
  now have room for 6 settings. Claude checked on the device that presses play without
  errors, and the user confirmed on 2026-11-08 that they work and are satisfying, in the
  menus and on the keyboard
- Boot jingle: Settings > GENERAL > BOOT JINGLE, ON (default, also for a settings file
  from before it) or OFF. Since 2026-11-10 it is matsukaze, the rush of a tetsubin before
  it boils, answered by two strikes of an iron wind chime (a Nambu furin, from the same
  ironware tradition), 1.7 s long, with a boot animation. The user chose it from 32
  candidates heard as WAV files (chip tunes, slower chip tunes, iron, furin), made by
  `scripts/jingles/make_jingles*.py` (numpy): chip tones with band-limited pulses, struck
  metal as decaying inharmonic modes with beating pairs, filtered noise and a small
  Schroeder reverb. The launcher keeps the chosen one as samples,
  `src/launcher/sounds/boot-jingle.s16` (48 kHz mono, 160 KB, peak 11000 of 32767),
  because its noise and reverb would not come out the same made again in Rust;
  `scripts/jingles/boot_jingle.py` writes it, and Claude checked it matched the chosen WAV
  to the sample. The first jingle, from 2026-11-09, was a spoon tapping an iron kettle,
  1.3 s, made as tones in `sounds.rs`.
  `S21jingle` runs `launcher --jingle` in the background just after `S20audio`, logging to
  `/var/log/jingle.log`, and writes its process to `/run/jingle.pid`. The launcher loads
  as usual, then waits for that process before it draws anything
  (`sounds::wait_for_boot_jingle`, at most 4 s), so a resumed game finds the card free.
  Turning the setting on plays the sound once, without the animation. It doesn't play
  when waking from sleep.
  The animation (`boot_animation.rs`) is drawn by the jingle's process over the kernel's
  logo, starting and ending on it exactly and drawing only where it is black (and the
  title): steam puffs from the kettle's spout as loud as the rush, a ring of teal blocks
  leaves the kettle at each strike (0.50 and 0.64 s), the title flashes, and a shine
  sweeps across it. It is in the logo's 5-pixel blocks, fading in steps of its colours,
  with a fixed random sequence so each boot looks alike. Its time comes from the samples
  the card has played (`hw_ptr` in `/proc/asound/card0/pcm0p/sub0/status`, read each
  frame while the state is RUNNING), so it waits out the amp's 700 ms and keeps to the
  speaker; with no sound by 1.5 s it runs on the clock. It draws every other refresh,
  about 30 frames a second, with `blit_canvas`: `Framebuffer::draw` took 31 ms a frame
  there, `blit_canvas` 7 ms, plus 4 ms to make the frame. `launcher
  --preview-boot-animation FILE.ppm` renders eight moments of it on the PC. Measured on the
  device on 2026-11-10: 50 frames during boot, the jingle's process ending 6.29 s after
  power on and the launcher drawn at 6.35 s (5.84 s with the first jingle, 3.83 s with it
  off). Claude saw steam, rings and the logo at the end in captures of the framebuffer
- Sleep when idle: Settings > SYSTEM > SLEEP WHEN IDLE: OFF (default), 2, 5, 10 or 30
  minutes without a press in the launcher's own screens (home, lists, settings), not in
  games, the Wi-Fi screen, the keyboard or a download (`Wake::Idle`, `sleep_until_woken`).
  It turns the backlight off through `bl_power`, leaves Wi-Fi as lid sleep does, and
  sleeps with `freeze`. The power button wakes it: only it and the lid are wakeup
  sources, and patch 0010 drops the waking press so it doesn't shut down. If the lid
  closes during it, that wakes it too, and it carries on as lid sleep. Saved as `idle=`.
  Claude checked it on the device at 2 minutes with an RTC alarm as the waker: it slept
  2 minutes after the last press (`PM: suspend entry (s2idle)`), woke 50 s later, turned
  the backlight back on and rejoined Wi-Fi. Not yet checked by the user: that the screen
  is dark while asleep, and waking with the power button. The test unit is back on OFF.
  It also powers off after POWER OFF AFTER SLEEP's time asleep on battery, like lid sleep (2026-11-10), with
  the same RTC alarm and timer; a power button press before then wakes it as before, and
  if the lid closes, the lid sleep keeps counting the same 2 hours. Claude checked it on
  the device with `LAUNCHER_LONG_SLEEP=60`, a fake `Discharging` battery and a fake
  `LAUNCHER_POWEROFF`: it slept at 2 idle minutes, the alarm woke it 61 s later and the
  power-off command ran. A real power-off from idle sleep hasn't been tried
- Wi-Fi and Bluetooth in the corner (2026-11-12), left of the clock and battery level on
  every screen with them (`corner::add_corner`, `Canvas::links`). Wi-Fi shows while
  `wlan0`'s `operstate` is `up`, that is connected; the kernel has no
  `/proc/net/wireless`, so there is no signal strength without asking wpa_supplicant.
  Bluetooth shows while it is on in the settings: dimmer with nothing connected, and as
  bright as the clock while a device is, which the kernel shows as an `hci0:<handle>`
  folder in `/sys/class/bluetooth`. Both are read from sysfs with the battery each
  second, and the menus draw again when they change. The Bluetooth rune is 9 pixel rows,
  centred on the 7-row text, since at 7 it reads as an asterisk. The home header's
  `CORNER_W` leaves room for both. Checked on the device: Wi-Fi connected, Bluetooth on
  with nothing connected, and turned off; a connected device wasn't tried
- Battery display: Settings > BATTERY > SHOW IN CORNER (in SYSTEM until 2026-11-13):
  PERCENT, TIME LEFT (see BATTERY below), WHEN LOW (only at LOW BATTERY AT or below
  while not charging, in red) or HIDDEN, for the menus' top-right corner; the clock takes
  its place. Games' low-battery warning and shutdown don't change. Like the accent, it is
  part of the `Theme`. Saved as `battery=`.
  Groups now have room for 5 settings (`MAX_GROUP_SETTINGS`). Claude checked all three on
  the device, WHEN LOW with a fake 9% battery through `LAUNCHER_BATTERY_DIR`; the test
  unit is back on PERCENT
- Covers in lists: Settings > BOX ART > COVERS IN LISTS, ON or OFF. Off leaves out the
  art panel, so names have the whole width, about 49 characters instead of 31. Saved as
  `covers=`. Claude checked the GBA list both ways; the test unit is back on ON
- Game names: Settings > THEME > GAME NAMES: SHORT (`names::display_names`, as before)
  or FULL, the file name without the extension, with its tags. Saved as `names=`;
  `build_library` takes the choice. Claude checked a list in FULL on the device; the test
  unit is back on SHORT
- Start screen: Settings > THEME > START SCREEN: HOME, RECENT, FAVORITES or LAST GAME,
  the list of the last game's console with that game selected, from the first line of
  `recent.txt` (`Launcher::start_in`, `open_start_screen` in `library.rs`). An empty list
  stays home, and a game resumed at boot returns to it. Saved as `start=`. Claude checked
  LAST GAME and RECENT on the device by restarting the launcher; the test unit is back on
  HOME
- Background shade: Settings > THEME > BACKGROUND: DARK (the first blue-grey), BLACK or
  TINT, an eighth of the theme colour over near black, which follows the colour
  (`settings::Background::rgb`). Like the accent, it is part of the `Theme`; it is
  also the dark text on selected rows. Saved
  as `background=`. Claude checked BLACK and TINT on the device; the test unit is back on
  DARK
- Grid size: Settings > THEME > GRID SIZE: NORMAL (4 to a row, every row shown) or LARGE
  (3 to a row with icons at 3x, two rows at a time, paging like the lists, with small
  arrows for rows above or below). `settings::GridSize` gives the columns and rows, the
  home screen and `grid_move` take them, and the settings grid stays at 4. Saved as
  `grid=`. Claude checked both pages of LARGE on the device; the test unit is back on
  NORMAL
- Hiding home screen tiles: Settings > THEME > HOME SCREEN TILES lists RECENT,
  FAVORITES, every console (with games or not) and SETTINGS, each SHOWN or HIDDEN, toggled
  with A (`Library::every_tile`, `screens::home_tiles_screen`). Saved as
  `hidden_tiles=recent,tamagotchi,...` in `settings.txt`, using each system's `key` in
  `SYSTEMS`. Games of a hidden console stay in RECENT and FAVORITES. If every tile is
  hidden, SETTINGS shows anyway; hiding it shows a reminder that Start opens settings.
  Fewer tiles can mean fewer rows, which makes the tiles larger. Claude hid and showed
  tiles on the device; the test unit has none hidden
- Time settings, in Settings > GENERAL (a TIME tile until 2026-11-13): TIME ZONE (58 cities, west to east, Left and
  Right) and CLOCK (24 HOUR, 12 HOUR or OFF) for a clock left of the battery level on
  every screen that shows it. The note under them shows the date and when the network last
  set the clock (from `/var/log/ntp.log`). Saved as `timezone=<IANA name>` and `clock=`
  in `settings.txt`; UTC until chosen. `clock.rs` holds each zone as a POSIX TZ rule,
  taken from the PC's tzdata file footers, so the device needs no zoneinfo: the launcher
  reads it with the `jiff` crate, and games get it as `TZ`, which glibc reads, so cores'
  clocks (Pokemon's RTC) show local time. A test checks every rule against the IANA
  database hourly for two years, and on the device glibc gave the same local times as
  the PC for all 58, in winter and summer. Claude checked the screens, the corner clock
  in both formats and off, and `TZ` in a game's environment. The zone is set to Berlin
  on the test unit. A game's clock was not checked inside a game
- Setting the time by hand (2026-11-12), Settings > GENERAL > SET TIME (`set_time.rs`), for a
  device without Wi-Fi. Day, month, year, hour and minute in boxes, Left and Right moving
  between them and Up and Down changing one, wrapping within it like a watch (a minute
  past 59 stays in the hour), the day kept within the month and the year between 2025
  and 2099. The hour follows CLOCK (4 PM in 12 HOUR), and the full date with its weekday
  shows below. It is local time in the chosen zone (`clock::zoned`: a time skipped when the
  clocks go forward counts as after the change, a repeated one as the first). A saves it
  with `clock_settime` and writes the RTC with `hwclock -w -u`, so it holds through a
  power-off; B leaves unchanged. Then it restarts ntpd (`S43ntpd restart`): a running
  BusyBox ntpd doesn't notice the jump and went on logging tiny offsets with the clock 2
  hours off, while a new one steps it on its first answer. So the network still wins,
  at once with Wi-Fi up or once it connects, which the screen says. Claude set 23:22
  instead of 21:22 on the device: the system clock, the RTC and the corner followed, and
  with the restart ntpd stepped it back within a second (-7175 s)
- Vibration (2026-11-10). The motor is on PWM channel 2, pin PH2, as in ROCKNIX's
  `0150-add-forcefeedback`; patch 0015 makes it a mainline `pwm-vibrator` at 1 kHz
  (`CONFIG_INPUT_PWM_VIBRA`), an input device with `FF_RUMBLE`. Before the patch, Claude
  set PH2 to `pwm2` with `devmem 0x0300B0FC 32 0x77777422` (PH config register, PH2 in
  bits 11:8) and ran the channel from `/sys/class/pwm`: the user felt pulses at 20, 35,
  50, 70 and 100 % duty. `rumble.rs` uploads one rumble effect through the `evdev` crate
  and changes its magnitude when a game asks (`RETRO_ENVIRONMENT_GET_RUMBLE_INTERFACE`,
  offered only when the motor opened). One motor stands for both: the weak one counts
  half, as `pwm-vibrator` does it, and any rumble runs at 20 % or more, up to the
  strongest for Settings > IN GAME > VIBRATION (OFF, LOW 40 %, MEDIUM 70 %, HIGH 100 %,
  `vibration=` in `settings.txt`; the note below it says it is one setting for every
  game). Changing it buzzes for 0.3 s at the new level. The motor is held still while
  the in-game menu is open, from a lid close until it opens, at power-off and when the
  game ends; closing the device file, also in a crash, makes the kernel stop it. mGBA
  rumbles for Game Boy rumble cartridges and the Game Boy Player, and changes the level
  almost every frame. In Pokemon Pinball (Rumble Version) that left the motor running
  for good, also after leaving the game: ff-memless clears an effect's PLAYING flag when
  new values are uploaded until its timer runs, and a stop in that window cleared the
  effect without telling the driver. The kernel still showed the channel enabled at
  26.6 % in `/sys/kernel/debug/pwm` with no game running. Patch 0016 aborts every
  started effect instead, and the user checked Pinball's strong rumble and leaving the
  game with it. PCSX ReARMed rumbles only
  with a DualShock, which `retro_set_controller_port_device` would select. Tried
  (2026-11-10): the core then switches the pad to analog by itself after a game's 16th
  configuration read (`pad.c`, "Auto-enabling dualshock analog mode", no option), and
  Metal Slug X took no input at all, while the plain pad works. So PlayStation games keep
  the plain pad and don't rumble. The user felt the setting's buzzes and told the levels
  apart. Adding the motor renumbered `/dev/input`, so `press.py` finds devices by name
- Settings is a grid of group tiles, like the home screen (`settings_grid`). Since
  2026-11-13 each group is one topic, so a setting is found where a user would look:
  GENERAL (volume, brightness, language, menu sounds, boot jingle, time zone, clock, set
  time), IN GAME (a DISPLAY page with scaling, effect, ghosting and border, vibration, the
  game switcher, and an ACHIEVEMENTS page), WI-FI and
  BLUETOOTH (their own screens), BOX ART, THEME (the menus' and home screen's look: colour,
  background, title, grid, tiles, start screen, game names, sorting), DATA (USB STORAGE,
  WI-FI TRANSFER, SAVE BACKUP: getting things on and off the device), SYSTEM (system info,
  button test, sleep, power-off after sleep, system update, reboot, power
  off), ADVANCED (SSH, root password, SSH keys, performance, boot messages,
  setup, reset), CORES, BATTERY and STATISTICS. Later on 2026-11-13 the TROPHIES tile
  became IN GAME's ACHIEVEMENTS page, the display rows IN GAME's DISPLAY page (`SUBGROUPS`
  in `launcher.rs`, whose B goes back to the row they came from, `Group::parent`), and
  BATTERY took its tile, with the battery display from SYSTEM and CHARGE TO from ADVANCED.
  Before, START SCREEN, GAME NAMES and
  BATTERY were in GENERAL, GAME SWITCHER in THEME, the sleep rows, USB STORAGE and WI-FI
  TRANSFER in SYSTEM, and the time rows in a TIME tile, where DATA is now. A opens a group's
  list, B goes back to its tile, and Start closes settings from anywhere
  in them. Groups and their settings are `GROUPS` and `Group::settings` in `launcher.rs`,
  so a new option is one more entry; `MAX_GROUP_SETTINGS` (9) is what the screen has
  room for, with no note; a group with a note has at most 8. Settings that can't be undone (delete covers, reboot, power off) need A
  twice. Group icons are in `icons.rs`. Claude checked each screen on the device; not yet
  seen by the user
- Settings > ADVANCED (2026-11-08):
  - SSH: OFF, USB ONLY (default) or USB + WI-FI, in `settings.txt` as `ssh=`.
    `S50dropbear` (in the overlay, replacing Buildroot's) reads that line itself at boot
    and when the launcher runs `restart` after a change: USB ONLY listens on
    `10.35.35.1:22` with `-B`; USB + WI-FI on every address without `-B`. dropbear can't
    allow an empty password on one network and not another, so USB + WI-FI needs an SSH
    key or a root password. Left and Right pass over it without one (`ssh::next_access`),
    the screen says why, and the script falls back to USB ONLY too. Restarting stops only
    the listener, so open sessions stay connected. Until this, dropbear listened on Wi-Fi
    as well, with root's empty password
  - ROOT PASSWORD: typed on the on-screen keyboard; Start on an empty field removes it
    (B on an empty field leaves without a change, which the user first took for
    removing it, so the line above the field says so). R1 fills in a
    made-up one (`ssh::random_password`): three groups of four from 27 lowercase letters
    and digits without lookalikes (no 0, o, 1, i, l, and no g, p, q or y, which the font
    draws without descenders, like 9, P and Y), about 57 bits, from `/dev/urandom`. It
    shows in the field to note down or edit, and Start saves it. BusyBox `mkpasswd -m
    sha512` hashes it from standard input. SSH KEYS: the count, and A twice removes them
    all. `scripts/push.sh key [FILE.pub]` adds this PC's key (`id_ed25519`, `id_ecdsa` or
    `id_rsa`) once. Both live in `/mnt/data/ssh` (`root-password`, `authorized_keys`), so
    updates and new images keep them: `S50dropbear` copies them to `/etc/shadow` and
    `/root/.ssh` at every start, which makes the password apply on the serial console too
    and replaces one set with `passwd`. `push.sh rootfs` overwrites `/etc/shadow`, so it
    restarts `S50dropbear` afterwards. Removing the last key or the password drops USB +
    WI-FI back to USB ONLY (`login_changed` in `app.rs`)
  - A release build (2026-11-14) allows no empty password anywhere. `post-build.sh`
    writes `VARIANT_ID=release` into `os-release` when built with `TETSU_RELEASE=1` (the
    release workflow does for a tag) and locks root's entry in `/etc/shadow` (`*`).
    `ssh::release()` and `S50dropbear` read that line. Then SSH defaults to OFF, the
    server never gets `-B`, every access needs a key or a password (`ssh::effective`
    falls back to OFF, and `S50dropbear` starts nothing), and without a password root
    stays `*`, so the serial console can't log in either. Right or Left on SSH while it
    is OFF with nothing set opens ROOT PASSWORD with a made-up one already in the field
    ("NOTE IT DOWN - START TURNS SSH ON"); Start saves it and turns SSH on one step from
    OFF (Right: USB ONLY), and B, which deletes first, leaves it off once the field is
    empty. The note under the group says TURNING SSH ON MAKES A ROOT PASSWORD, and THE
    USB CONSOLE STILL WORKS only with a password set. Removing the password and keys
    turns SSH off again
  - Claude checked on the device (2026-11-14), with the device's `os-release` marked as
    a release and the new launcher and `S50dropbear`: with a key, SSH ran on USB without
    `-B`, the key logged in and the empty password was refused; without the key the
    server stopped and SSH showed OFF with the note; Right showed a made-up password,
    Start turned SSH to USB ONLY (the first try went to USB + WI-FI, from the old `usb`
    setting: fixed by stepping from what was in effect), the shown password logged in
    and the empty one was refused; removing the password turned SSH off and locked root
    again, and the serial console refused root ("bad salt"). Then put back to a
    development build. A whole release image on the device is not yet checked
  - PERFORMANCE (`performance.rs`, `performance=`): BATTERY is `schedutil` up to 1104 MHz,
    BALANCED (default) `schedutil` up to 1512 MHz, as before, and PERFORMANCE the
    `performance` governor at 1512 MHz. The launcher sets them at start and on a change;
    one cpufreq policy covers all four cores. The in-game menu's 1008 MHz hold stays below
    every cap
  - Claude checked on the device: OFF stops the server and USB ONLY brings it back; USB
    + WI-FI is skipped with nothing set; with a key it listens on `0.0.0.0:22`, the key
    works over Wi-Fi and USB, and an empty password is refused on both; a password typed
    on the keyboard logs in over both and a wrong one is refused; after a reboot the
    hash, keys and Wi-Fi mode came back; removing keys kept Wi-Fi (password still set),
    and removing the password then fell back to USB ONLY with the empty password working
    again; each PERFORMANCE level set the governor and cap. Not checked: a game at
    BATTERY or PERFORMANCE, and how either feels or lasts
  - CHARGE TO (`charge=`, in BATTERY since 2026-11-13): FULL (4.2 V), ABOUT 85% (4.1 V) or ABOUT 70% (4.0 V), the
    voltage the AXP717 stops charging at, written to the battery's `voltage_max`
    (`power::apply_charge_limit`). The driver takes only these three. The PMIC keeps the
    value until the battery is disconnected, so the launcher writes it at every start;
    this unit's PMIC was at 4.2 V, so FULL changes nothing. The percentages are rough,
    for a typical cell. Charging current stays at the device tree's 1024 mA, which is
    also the driver's maximum. A software stop at a percentage (turning charging off
    through `status`) was left out: a crash could leave charging off. Claude checked
    that each value reached `voltage_max` and that FULL started charging again; the user
    saw the charge level behave so far, but not yet over a whole charge. The orange light
    can blink after choosing 85% or 70% with the battery above that limit. While it
    blinked at 85% (4.16 V, USB in), `0x01` read `0x95` and `0x4a` had bit 0 set (battery
    over-voltage), with health Good and `0x02` clear. Writing 4.2 V and then 4.1 V gave a
    few seconds of charging, then `0x94` and no over-voltage bit, and the blinking was
    found stopped. Unplugging and replugging USB after that gave one blink only, with the
    same readings. The cause is inferred: no AXP717 charger or LED documentation was found
    (the AXP717D datasheet has neither), and what starts the blinking wasn't reproduced.
    No harm was seen. `current_now` has an unknown offset, so about 425 mA here isn't real
  - BOOT MESSAGES (`boot_partition.rs`): LOGO is `console=ttynull loglevel=5`, as before;
    TEXT is `console=tty1 loglevel=7`, so kernel and init messages show until the
    launcher draws. The launcher changes only those arguments on the `APPEND` line of
    `/boot/extlinux/extlinux.conf` in the root filesystem (on TETSU-BOOT until
    2026-11-12), writes it as `extlinux.new`, reads it back and renames it over the old
    one. The value is read when ADVANCED is drawn. `push.sh boot` and flashing put
    back LOGO. Claude checked that TEXT and back gave the original file byte for byte,
    and that the device boots with TEXT (launcher drawn 1.48 s after the kernel started,
    against 1.38 s); the user saw the logo with the messages below it, as it should be
  - RESET SETTINGS (A twice): `Settings::reset` keeps volume and brightness and sets
    everything else to its default, so SSH goes back to USB ONLY, the time zone to UTC
    and the charge limit to FULL; the CPU, charge limit and SSH server are applied again.
    Games, saves, covers, Wi-Fi networks, keys, the root password and the boot messages
    stay. Checked on the device, then the user's settings were put back
- SAVE LOGS TO CARD, X on Settings > SYSTEM > SYSTEM INFO (`boot_partition::save_logs`): writes
  `launcher.log`, `dmesg.txt` and `system.txt` (date, device, kernel, build, uptime,
  settings, storage, RAM type and voltage, memory, processes, network) to `logs` on TETSU-BOOT, replacing the last ones, and
  says where. Wi-Fi passwords, keys and the root password hash are in other files and
  left out. Checked on the device
- System info (`system_info.rs`) shows 11 lines, as many as the screen has room for:
  battery with voltage, game storage, game count (a `.cue` and its `.bin` count once),
  the RAM as sold with its type and what is free (`1 GB LPDDR3 - 856 MB FREE`; free of the
  kernel's total if the type is unknown), CPU speed and temperature, the Wi-Fi network and address (from
  wpa_supplicant's `STATUS`), the USB address, time since boot, kernel release and build
  date, the system build and the app version. The system build is the project's git
  revision, which Buildroot writes to `VERSION` in `/etc/os-release`; MODIFIED means
  it was built with uncommitted changes. It reads once per redraw (a key press, a new
  minute or a battery change). Claude checked the screen on the device
- Box art over Wi-Fi, from Settings > BOX ART (`boxart_fetch.rs`), with the same sources
  and matching as the script, which still works over USB. DOWNLOAD MISSING COVERS fetches
  games without one, DOWNLOAD ALL COVERS AGAIN replaces each cover once its new one is
  saved, and DELETE ALL COVERS removes every `.ppm`, including those of removed games.
  The group shows how many games have a cover. Each console's collection is
  `thumbnails` in `SYSTEMS`. The app downloads over HTTPS itself (`boxart_fetch::Https`:
  `ureq` with rustls, HTTPS only, checking certificates against the Mozilla roots built in
  by `webpki-roots`, so the device has no curl or certificate files since 2026-11-08);
  `serde_json` reads GitHub's file listing; the `png` crate decodes (`png.rs`); covers are
  shrunk by area averaging. After the switch from curl, four covers downloaded again were
  byte-identical to curl's. The screen shows progress, B stops after the current game, the lid stops the
  download before sleeping (sleep leaves the network), and games with no cover are listed.
  The PNG tests use images from `src/launcher/testdata/make.py`. Claude checked all 38
  cached covers decode exactly as Pillow does, then on the device: all 38 downloaded in
  about a minute, at the script's sizes with a mean difference of at most 4.2 of 255
  (Lanczos against area averaging), the closest-name fallback, a game with no cover, B
  while running, and not connected. Then again after the settings rework, with the `png`
  crate: delete all, download missing (38) and download all again (38). The lid during a
  download is untested

- Display options, in Settings and the in-game menu, saved in `settings.txt`, after
  research into emulator shader techniques. Scaling is sharp-bilinear (`axis_weights` in
  `libretro/scale.rs`): a screen line across a game pixel boundary blends the two by coverage,
  so pixels look even at non-whole FIT scales. SCALING INTEGER uses whole multiples: NES
  512x480, SNES 512x448, Game Boy 480x432, GBA 480x320. SCREEN EFFECT CRT darkens the gap
  between game rows by up to 75% for dark pixels and 25% for bright ones, brightened 1.25
  times; LCD is a soft grid, brightened 1.12 times. Both use coverage of the pixel gap
  (`gap_coverage`), so they stay smooth at any scale. The gain doesn't clip
  (`Highlights`, 2026-11-11): once a pixel's brightest channel passes 216, it bends to
  reach 255 at the gain's top, and the other channels follow it, so highlights keep
  their detail and colours their hue (before, Super Mario Bros.' sky turned grey-cyan as
  its blue clipped). Colours with a channel near full are brightened less than before,
  and their scanlines are fainter. The effect is laid over each line as it is scaled,
  the tables are kept in a `Scaler` until the sizes or effect change, and the last
  result is kept along a line. On the device, a benchmark scaling real NES (512x480) and
  Game Boy (533x480) screenshots took 3.35 to 3.48 ms with CRT against 4.21 to 4.23
  before, and 2.79 to 2.83 ms with LCD against 3.12 to 3.19; with no effect, about the
  same (1.35 to 1.57 ms). Random noise, the worst case, is slower: CRT 5.7 against 4.9 ms.
  LCD GHOSTING sets mGBA's interframe
  blending: OFF, SMART (default, only flickering pixels) or FULL, applied live through
  `GET_VARIABLE_UPDATE`. Always on: mGBA colour correction (`Auto`) and FCEUmm's measured
  `smooth-v2-fbx` NES palette. The old `scanlines` and `grid` names load as CRT and LCD.
  FireRed with CRT: scaling 4.6 ms per frame at 59.4 to 59.6 fps. Moving drawing to the
  GPU would mean replacing fbdev with DRM, so it wasn't done. Claude checked each effect
  in screenshots of FireRed run from copies of its saves, and the user confirmed they
  look much better
- The sound probe never lowers the samples per frame below the core's timing. Starting
  games make less sound: LeafGreen measured 1065 against 1097, and lowering the rate made
  it wait on the audio at 58.1 fps. FireRed now plays at its full 65536 Hz
- 50 Hz games at their own rate (2026-11-14, `ui/panel_rate.rs`). A game whose core runs
  at 45 to 55 fps (PAL games, Vectrex, Virtual Boy) sets the panel to the nearest rate it
  can make, instead of 59.94 Hz, where some frames showed twice. Settings > IN GAME >
  DISPLAY > 50 HZ GAMES AT 50 HZ turns it off (`50hz=off`, `fifty_hz_off`; on by default,
  also for an older settings file); it is the last row, after USE FOR ALL CONSOLES, since
  it is one setting for every console, and its note says so. The `--play` process
  opens `/dev/dri/card1`, becomes DRM master and sets a mode with `SETCRTC` on the first
  CRTC with `fb_id` -1, which keeps the framebuffer fbdev made, so drawing into
  `/dev/fb0` doesn't change. The pixel clock stays 24 MHz (`pll-video0` at 72 MHz, which
  HDMI also uses); only the porches grow, from the panel's 770 x 520 up to at most
  800 x 700, before each sync pulse. The panel runs in DE mode (`0x23 = 0xa0`), so it
  takes them. Found rates: PAL SNES and NES 50.007 fps on 792 x 606 (50.005 Hz), Mega
  Drive 49.7015 on 789 x 612 (49.703), PlayStation 49.7607 on 783 x 616 (49.759), 50.000
  on 800 x 600. The mode change turns the panel off and on, 162 ms, at the game's start
  and end. Closing the device, also when the process ends or crashes, makes the kernel
  restore its own mode (lastclose); the menus process holds only `/dev/fb0`, which this
  needs. While a master exists, the kernel refuses `FBIO_WAITFORVSYNC` with EBUSY
  (`drm_client_modeset_wait_for_vblank`), so the game waits with `DRM_IOCTL_WAIT_VBLANK`
  on the same device. The sound's rate uses the panel's real rate (`playback_rate`), and
  the TCON registers (`0x651104c`, `0x6511050`: totals, the lines doubled) show the mode
  with `devmem`. Checked on the device with Pac-Man (Europe): 50.005 Hz set, the TCON at
  792 x 606, 59.94 Hz again after QUIT, and 49.92 fps steady, the sound's 0.1% margin
  making the game wait on the sound about once every 12 s (a repeated frame). With
  the setting OFF, the same game left the panel at 520 lines and ran as before. A test
  program measured 50.033, 49.953 and 49.715 Hz from vblank times for 623, 624 and 627
  lines. While the game holds the display, the kernel doesn't light a newly plugged HDMI
  TV until the game ends; one plugged in before keeps 59.94 Hz

- Fast-forward: holding R2 runs extra frames in each panel refresh, within 6 ms and up
  to 8 frames, keeping only the last frame's picture and sound, so sound keeps flowing.
  A `>>` shows in the top-right corner, and the CPU is held at 1008 MHz meanwhile. A Game
  Boy game reached about 5x with no underruns; at an 8 ms budget it missed refreshes and
  the sound ran out. The launcher log's statistics show the speed. The user confirmed it
  works; the sound is very choppy, especially on NES, which the user accepts
- Sound rate from measured samples. The first 30 frames run silent while the frontend
  counts samples per frame, then aplay starts at the matching rate (`playback_rate`).
  mGBA reports 61.17 fps for Super Game Boy games but makes sound as at 59.73, so the
  rate from its timing made them wait on the audio at 58.48 fps. They now run at 59.5

- Resuming at boot. Starting a game writes its path to `/mnt/data/playing.txt`; quitting
  or a crash removes it, but shutting down mid-game stops the launcher before it can, and
  an empty battery keeps it on purpose. At start, the launcher shows RESUMING with the
  game for 1.5 seconds, where B cancels, then starts it, and its resume state continues
  it. Claude tested quitting, cancelling, and a real reboot mid-game, after which the
  game came back by itself. The user confirmed it works with the power button

- Low-battery protection (`power.rs`). Running on battery, a game shows the charge in red
  in the top-left corner, which is border for every system, at Settings > BATTERY > LOW
  BATTERY AT or below (10% until 2026-11-13, now 15% by default). At 3% or
  below, or under 3.35 V, read twice 5 seconds apart, the game saves as when quitting,
  shows BATTERY EMPTY and exits with code 3, and the launcher powers off. The launcher
  also powers off when it reads empty. Closing the lid in a game saves its resume state,
  and the launcher waits 500 ms before sleeping. `LAUNCHER_BATTERY_DIR` and
  `LAUNCHER_POWEROFF` point the battery reading at test files and replace `poweroff`.
  Claude tested the warning, the empty path and the lid save on the device with those;
  a real empty battery hasn't been tried

- Settings > BATTERY (2026-11-13): CHARGE HISTORY, HEALTH AND USE, SHOW IN CORNER, LOW
  BATTERY AT, CHARGED NOTICE and CHARGE TO. The AXP717 driver gives the level, voltage
  and status, but no capacity, cycle count or temperature, and its `current_now` has an
  offset the driver itself calls unknown (it read 429 mA at 100% not charging), so all of
  this comes from a log of the level over time.
  - The log (`device/battery_log.rs`): the menus' process, which stays up through games and
    puts the device to sleep, samples every 60 s, and at once on plugging in, unplugging or
    charging ending (a thread looks every 10 s). A line is `time percent mV state activity`
    in `/mnt/data/battery-log.txt`: state `D` discharging, `C` charging, `K` charging at
    constant current with a good supply, `F` on USB power and not charging; the activity
    is what the device did since the sample before: `menu`, `sleep`, a console's key, or
    `off` (powered off, or the launcher not running). `K` reads the charge phase in REG
    0x01 bits 0-2 (2 constant current, 3 constant voltage, 4 done) from the regmap's
    debugfs listing, only while charging, and needs USB at 4.5 V or more. Samples stay in
    memory and are appended every 15 minutes, at a game's start and end, before sleep and
    at shutdown (also the power button's and the sleep timer's), only while TETSU-DATA is
    mounted read-write (`storage::data_writable`) and not while USB STORAGE has it
    (`battery_log::hold`). `sleep::freeze` wraps every sleep, so the time asleep is one
    `sleep` sample taken on waking. Samples older than 30 days go at start, once a day
    past that, into `/mnt/data/battery-stats.txt` (`since`, `charged` percentage points,
    `capacity` measurements), so cycles and capacity outlast the log. The log isn't part
    of SAVE BACKUP: it belongs to this battery
  - Estimates (`device/battery_estimates.rs`): each activity's drain is the percentage
    points lost over the time between discharging samples (gaps over 3 minutes awake and
    `off` count for nothing; a sleep counts whole), given once there is half an hour and 2
    points (2 hours for sleep). Time left is the level above 3% over the current
    activity's rate, or all awake time's; the corner's TIME LEFT shows it in the menus
    ("80% 4H05"), and only the percentage until there is enough. Time to full adds each
    remaining point at the speed earlier charges had in that tenth of the range. Charge
    cycles add every rise while on USB power, 100 points a cycle. Capacity: a run of `K`
    samples of at least 20 minutes and 20 points gives the set charge current
    (`constant_charge_current_max`, 1024 mA) times its time over the points gained; the
    capacity shown is the median of the newest 5, against the profile's rated 3300 mAh.
    This assumes the gauge's percentage follows the charge linearly and the charger
    really delivers its set current in that phase; neither is checked yet
  - CHARGE HISTORY (`device/battery_screen.rs`): the status line (level, state, voltage),
    the time left or to full, and the level over 24 hours or 7 days (Left and Right), in
    the accent on the battery, green plugged in, grey asleep, and a gap while off, with
    ticks every 6 hours or each midnight in local time. HEALTH AND USE: capacity, cycles
    since the log began, when charging last ended, and BATTERY USE: the menus, each console
    and sleep, with points per hour and how long a full charge lasts at it (Up and Down
    scroll). `launcher --preview-battery day|week|health FILE.ppm` renders them from a
    made-up log
  - LOW BATTERY AT (`low_battery=`): 5, 10, 15 (default) or 20%, where the corner turns
    red, WHEN LOW shows, and a game warns (`power::level`)
  - CHARGED NOTICE (`charged_notice=`, on by default): BATTERY CHARGED as a sign at the top
    in the menus and in a game when the status goes from Charging to anything but
    Discharging (`power::ChargeWatch`)
  - Checked by Claude on the device: the log starting and sampling, its first write to the
    card, the BATTERY tile, its group and both pages (with only minutes of log), IN GAME's
    DISPLAY and ACHIEVEMENTS pages and B from them. Not checked: anything on battery
    (time left, drain rates), a real charge (time to full, `K` samples, the capacity
    measurement and whether it is near the truth), sleep samples, the CHARGED sign, the
    30-day trim, USB STORAGE with the log, the German wording

- The menu checks the battery every second and redraws when it changes. Before, charging
  status only showed after moving in the menu, or after 30 seconds

- Wi-Fi, set up from Settings > WI-FI. The chip is a Realtek RTL8821CS on SDIO
  (`024C:C821`, `mmc1`), driven by `rtw88` with `rtw8821c_fw.bin` and the regulatory
  database built into the kernel, because the driver probes at 0.5 s, before the root
  filesystem. `external.mk` points `CONFIG_EXTRA_FIRMWARE_DIR` at the target, so
  `post-build.sh` keeps that one firmware file there. `S42wifi` starts `wpa_supplicant`
  and `wpa_cli -a /usr/libexec/wifi-event` in the background, with networks in
  `/mnt/data/wpa_supplicant.conf`; the hook gives each connection its address (below).
  The launcher talks to wpa_supplicant's control socket (`wifi.rs`): the screen
  (`wifi_menu.rs`) lists networks in range by signal, with saved ones out of range
  last, scans every 10 s and on Y, joins with A, and opens a saved network's page with
  X (below). A
  password is typed on an on-screen keyboard (`keyboard.rs`) with lowercase and every
  printable symbol (`font::glyph_cased`, used only where case matters). A new network
  is saved only once it connects.
  WPA3-only, enterprise and WEP networks show as NOT SUPPORTED, since WPA3 would need
  OpenSSL. Claude saw the list of 9 real networks and the keyboard on the device. Sleep
  works with Wi-Fi (patch 0013), checked with an RTC wake alarm
  (`echo +8 > /sys/class/rtc/rtc0/wakealarm; echo freeze > /sys/power/state`). The user
  joined their network on 2026-11-08, and it came back after a reboot. Lid sleep while
  connected hung the device until the launcher started disconnecting first; with that,
  an RTC-timed sleep woke and rejoined within 8 s, and the user confirmed lid sleep
  works and Wi-Fi reconnects
- More Wi-Fi settings (2026-11-12), all under Settings > WI-FI:
  - An ON/OFF row first (`wifi=` in `settings.txt`). Off runs `S42wifi stop`: no
    wpa_supplicant and `wlan0` down, also at boot, and the corner shows no Wi-Fi.
  - A saved network's page (`wifi_pages::NetworkPage`), from X, or A on the connected
    one: CONNECTION DETAILS (address and prefix from `ip`, gateway from
    `/proc/net/route`, DNS from `resolv.conf`, MAC, signal, band and channel and speed
    from `SIGNAL_POLL`, security), IP ADDRESS AUTOMATIC or MANUAL with ADDRESS
    (`192.168.1.50/24`, or /24 without a prefix) and GATEWAY, DNS (up to 3, empty for
    the router's), AUTO-JOIN, MAC ADDRESS DEVICE or PRIVATE, and FORGET (A twice).
    MANUAL without an address opens the keyboard with the address and gateway DHCP gave.
    Values are checked as typed (`network.rs`), and each change is saved at once.
  - Where they are kept: wpa_supplicant's own network block, so forgetting removes
    them. The address settings are its `id_str`
    (`ip=static;address=..;gateway=..;dns=..;join=manual`, `network::NetworkConfig`),
    which `wpa_cli` passes on: `/usr/libexec/wifi-event` sets a static address, route
    and DNS (the gateway's without servers of its own), or runs `udhcpc` with
    `/usr/libexec/wifi-dhcp`, which puts the network's servers in place of the router's.
    It also applies POWER SAVING, and logs to `/var/log/wifi-event.log`. Leaving the page
    applies changed address settings with `wifi-event wlan0 REFRESH`, without
    reconnecting.
  - AUTO-JOIN off is `disabled=1`. Choosing a network turns the others off in
    wpa_supplicant, so instead of `ENABLE_NETWORK all` each save turns each network on
    or off by its own setting (`Control::save_config`); turning off the connected one
    doesn't leave it. Lid sleep rejoins the network it left even with AUTO-JOIN off.
  - PRIVATE is `mac_addr=3` with a `mac_value` made up when it is chosen (local,
    unicast); wpa_supplicant shows it only as `*`, and 2.12 didn't save it, so patch
    `wpa_supplicant/0001`. rtw88 changing its address while Wi-Fi ran left the link
    associated but sending nothing two times in seven (only `S42wifi restart` brought
    it back), so a MAC change restarts Wi-Fi instead, six of six clean, and a new
    address on each connection (`mac_addr=1`) isn't offered.
  - Start opens WI-FI OPTIONS (`wifi_pages::WifiOptions`): HOSTNAME, COUNTRY, POWER
    SAVING and ADD A HIDDEN NETWORK (name, then password, empty for open;
    `scan_ssid=1`). The hostname (`hostname=`, lowercase letters, digits and inner
    hyphens) is set at boot by `S16hostname` after TETSU-DATA is mounted, with
    `/etc/hosts`; changing it runs `S16hostname reload`, which renews DHCP with the new
    name and restarts `mdnsd`. `mdnsd` (Buildroot's, with its SSH service) answers to
    `<hostname>.local` on Wi-Fi and USB; from the PC it resolved over both, the router
    passing it on over the LAN. COUNTRY (`wifi_country=`, `network::COUNTRIES`, 55 by
    name in the language in use, AUTOMATIC first) is set by `S42wifi` with `iw reg set`,
    `00` for AUTOMATIC, after which the router's own country still applies. While
    connected, a string of changes left cfg80211 with an intersection of them (country
    98), so a new country applies on leaving the page by restarting Wi-Fi. POWER SAVING
    (`wifi_power_save=`) is `iw dev wlan0 set power_save`, at once and on each connect.
    RESET SETTINGS puts all four back and applies them.
  - Init scripts read these with `/usr/libexec/setting KEY DEFAULT`, which also falls
    back to `settings-backup.txt`.
  - Checked on the device by Claude: on and off, the page and details, a static address
    (pinged from the PC), DHCP with own DNS, AUTO-JOIN off saved as `disabled=1`,
    PRIVATE and back with a restart each, a hidden open network that failed after 30 s
    and was removed, forgetting, a hostname change (DHCP and `.local`), Germany and
    Vietnam and back to AUTOMATIC, power saving off, and all of it after a reboot. Not
    checked: lid sleep with AUTO-JOIN off, a real hidden network, the German wording on
    the device

Not working yet:

- A faint high-pitched noise with USB connected. VBUS read 4.42 V at a 2 A input limit on
  a PC port, so it may be overloading the port. Not heard as of 2026-11-08, so not
  diagnosed. The user will say if it returns

- USB host mode is disabled, so USB accessories don't work
- The wrong-password message over Wi-Fi is untested
- An occasional single audio underrun during play, sometimes at the start, seen in
  `/var/log/launcher.log`. It was there without the menu. Not yet investigated

The kernel is trimmed to the Allwinner platform with modules off, and the user confirmed
that it boots and that everything still works, including power-off. The image went from
40.6 MB to 19.8 MB, and the root filesystem contents from 94 MB to 7.9 MB. The kernel now
reached `init` at 2.16 seconds, down from 2.86. Removing `console=ttyS0` from
`extlinux.conf` then brought that to 0.52 seconds, because sending messages to that port
at 115200 baud took most of the time. With a serial adapter, add it back to see kernel
messages on that port.

The root filesystem contents went from 255 MB to 40 MB. Buildroot's Panfrost option forces
LLVM, and through Mesa's OpenCL option Clang and libclc, but Panfrost's shaders are
precompiled on the PC. `external.mk` builds Mesa with `-Dllvm=disabled`, and
`post-build.sh` deletes the unused libraries and tools, then fails the build if anything
links to a missing library. Claude checked over SSH that `kmscube` still runs at
59.94 fps on Panfrost. The rootfs partition is still a fixed 1 GiB.

- **Translations** (2026-11-09): Settings > GENERAL > LANGUAGE switches the launcher and
  the in-game menu between English and German at once, with no restart. It is kept in
  `settings.txt` as `language=en` or `de`, and RESET SETTINGS leaves it, so the menus stay
  readable to whoever is using the device. `i18n::tr("SETTINGS")` looks the English text
  up in a language file, and `i18n::trf("{} OF {}", &[&a, &b])` also fills in values, in
  order; a text with no translation stays English. Screens ask as they draw, which is why
  the switch is immediate. Each language is a plain text file built into the launcher
  with `include_str!`, `src/launcher/lang/de.txt`, in lines of `ENGLISH = TRANSLATION`, so
  there is nothing to go missing on the device. To add a language: add the file, a variant
  in `settings::Language` and a line in `i18n::table`. Console names, TETSU OS and units
  stay as they are, and the language names are never translated.
  The font has one fixed width and everything is in capitals, so `[max N]` sections in the
  file give the room each kind of text has: 11 characters on a tile, 53 on a full line.
  That is why German says OPTIONEN rather than EINSTELLUNGEN, as the user chose short
  wording over changing the layout. The tests in `i18n.rs` fail on a text on screen with
  no translation, an entry nothing uses, a lost `{}`, a character the font can't draw and
  a text too long for its section. They find the English keys by reading the source for
  `tr("...")`, plus the lists of labels in `keys_elsewhere`, so text in a new list of
  labels has to be added there. Deleting a line, an overlong tile and a character without
  a glyph each failed as they should. They can't notice a string that was never wrapped in
  `tr`, which needs a look at the screen.
  The font gained Ä, Ö, Ü and ß, which also fixes these letters showing blank in game and
  network names. Other accented letters are still blank. Ä, Ö and Ü are the plain capital
  with two dots drawn two rows above the line (`font::dots_above`): squeezed into the
  font's 7 rows with their dots, they looked lowercase beside other capitals, as in
  `ZURüCK`. Lowercase ä, ö and ü, for network names, fit their dots in the glyph.
  `LAUNCHER_PREVIEW_LANGUAGE=de` renders the `--preview*` screens in German on the PC.
  Tested on the device on 2026-11-09 by switching with the buttons and screenshots of each
  settings group, the settings grid, Wi-Fi, system info, the home tiles list, a game list
  and a confirmation, then a launcher restart; `launcher --preview-pause` on the device
  showed the in-game menu reading the language from the file. The translation was written
  by Claude, and the user checked the wording and approved all of it on 2026-11-09. Not
  seen on the device by Claude: USB
  STORAGE's screens, the box art download, the keyboard, full-screen messages such as
  powering off, and the in-game menu inside a running game.
  A full-screen message drops from the large text to a smaller size when it wouldn't
  fit (`screens::message_screen`), which some English ones already needed: `THE CARD IS IN
  USE - TRY AGAIN` ran off both sides. Checked by a unit test, not seen on the device.

- **Setup on a new install** (2026-11-09): before the first home screen, four steps on one
  screen each (`setup.rs`, drawn by `screens::setup_screen`): LANGUAGE, WI-FI, TIME and
  GAMES. Up and Down on the language step switch the language at once. WI-FI opens the
  normal Wi-Fi screen and GAMES opens USB STORAGE, and coming back from either selects the
  way on. TIME has the time zone and the clock format, and shows the time as it will read.
  A goes on, B goes back, and Start leaves from any step, because everything chosen here is
  also in Settings. Choosing German preselects Berlin when no zone is set yet; English
  stays on UTC. Settings > ADVANCED > RUN SETUP AGAIN starts it again.
  It runs when `settings.txt` doesn't exist (`settings::from_file`) and neither does
  `settings-backup.txt`, which only a new install has. A file from before the setup has no `setup` line, so existing devices don't get it
  after an update, and an unreadable file doesn't ask either. While it runs the file has
  `setup=pending`, removed on finishing, so powering off partway starts it over, with the
  choices so far kept. RESET SETTINGS doesn't start it. If TETSU-DATA can't be mounted the
  file is also missing, so the setup shows on each boot until the card is fixed.
  The screen has no clock in the corner, as the time may be wrong until the network sets
  it. It doesn't sleep when idle, like the Wi-Fi screen; the lid, the volume buttons and
  powering off on an empty battery work as elsewhere.
  `launcher --preview-setup 0..3 FILE.ppm` renders a step on the PC. Tested on the device
  on 2026-11-09 by removing `settings.txt` with the launcher stopped and restarting it,
  with screenshots: language switching, the Wi-Fi screen and its way back, Berlin
  preselected, changing the zone and clock, a restart partway coming back to the first step
  with `setup=pending` and the choices kept, Start finishing it, no setup after the next
  restart, and RUN SETUP AGAIN. The user then went through it on a card with an empty
  TETSU-DATA, the same day, and the logs showed: setup on screen 5.77 s after power on, a
  Wi-Fi network joined and saved, the clock set by ntpd, USB STORAGE ejected by the PC with
  a clean check of the card, and the setup finished. The user confirmed that the game count
  went up after USB STORAGE, the menus were silent with menu sounds off, the setup stayed
  away after a reboot, and its German wording reads well. Not tried: the lid during it.

- Screenshots. SCREENSHOT in the in-game menu saves the paused picture as a PNG in
  `/mnt/data/screenshots` (`screenshot.rs`, `png::encode`), and the menu stays open, so
  it can be followed by anything else. The file holds the game's own pixels at the core's
  size, without scaling, screen effects, the menu or any sign: `video_refresh` keeps a copy of each
  frame it shows (`Host::source`), as a core's pointer is only valid during the call. That
  copy runs on every frame; its cost hasn't been measured, and is a memory copy of at most
  about 600 KB (PlayStation at 640x480). Consoles with non-square pixels, such as the SNES
  and PlayStation, are saved as their pixels are, so a PC shows them a little off the 4:3
  shape they have on the device.

  Files are named after the game's save file and a number: `Tetris.gb 001.png`. The save
  name already tells `Tetris.gb` from `Tetris.nes`, stays the same when a game is zipped,
  and suits grouped discs (`G.m3u 001.png`). A number and not a time, because the clock can
  be wrong before the first network time. The next number follows the highest there, so an
  older picture is never replaced.

  A game with no cover shows its newest screenshot in lists and on its details screen
  (`BoxArt::get`), unless Settings > BOX ART > SCREENSHOTS AS COVERS is OFF (on by default,
  saved as `screenshot_covers=`), which shows the console's icon instead. It's shrunk to the 200x300 of a cover by the same averaging and given the
  console's shape. It is read at display time and never copied to `boxart`, so BOX ART still
  counts that game as having no cover, DOWNLOAD MISSING COVERS still fetches one, and a real
  cover then takes over. DELETE ALL COVERS leaves screenshots. So does deleting a game, as
  DELETE GAME AND SAVES says nothing of them and they are the player's own pictures; a game
  put back later shows its old screenshot as a cover. Screenshots can be taken from the
  card with USB STORAGE or copied over SSH.

  The menu gained a row, so its panel grew to 450 lines (`PAUSE_PANEL`), keeping the row
  height. Claude tested on the device on 2026-11-09 with Super Mario Bros. Deluxe (160x144)
  and Super Mario World (256x224), with their covers moved aside. Saving took 0.02 to 0.03 s
  and files of 3 to 10 KB. Two shots in a row got 001 and 002. The World shot had no CRT
  lines with the effect on, and both looked right on the PC. Each showed as the cover right after
  quitting, World at 200x150; putting its real cover back replaced it on returning from the home screen.
  A file in the folder's place gave COULD NOT SAVE THE SCREENSHOT with the game still running.
  German was checked in the `--preview-pause` picture. Saves, RECENT and covers
  were put back and the test pictures removed. SCREENSHOTS AS COVERS was then flipped on the
  device with a game whose cover was moved aside: OFF showed the console's icon and ON the
  screenshot, each after going through the home screen. The other consoles, including PlayStation's
  wide modes, a ZIP and a full card, are covered by unit tests at most, or not at all

- The screenshot gallery (`library/gallery.rs`). A SCREENSHOTS tile on the home screen,
  before SETTINGS, opens every picture in `/mnt/data/screenshots`; it is there only while
  there are any, and HOME SCREEN TILES can hide it like the others (`screenshots` in
  `hidden_tiles=`). It has no count under it, as the tiles count games. A game's details
  screen has a SCREENSHOTS row with that game's count, only while it has some, which opens
  the gallery with just its pictures. The details screen's facts and rows moved closer
  together (22 and 26 lines) so its five rows and the delete question still fit above the hint.

  The gallery shows thumbnails 4 by 3, newest first by the file's time, a game's own by
  their number when the times are the same (FAT keeps them to 2 seconds). Each is drawn in
  its console's shape, as covers are, in a 136x102 cell; a file that can't be read shows
  its console's icon. Below the grid are the game's icon, its name as the library has it
  and the number from the file (SUPER MARIO BROS. 001), and the place among them (3 OF 14).
  A file not named the menu's way shows its own name and a camera icon, and a game no
  longer on the card its file's name. The arrows move, L1 and R1 go a page; A shows the
  selected one large, up to 600x322 and enlarged with the same averaging as covers, where
  Left and Right step through all of them and B goes back to the grid. Y and then A deletes
  the selected one (PRESS A TO DELETE THIS SCREENSHOT; anything else calls it off); the
  next one is selected, and deleting the last leaves the gallery. Thumbnails are read when
  their page shows and kept while the gallery is open. Leaving after a delete reads the
  folder again and forgets the covers, so a game whose cover was that picture shows its
  next newest or its icon. The gallery is its own loop, like SET TIME, opened by
  `run_launcher` because it names the games from the library. Matching a picture to its
  game only asks games whose file is named like it, as a ZIP's save name means opening it;
  each game's name is remembered in `Library::screenshot_bases`.

  Claude tested on the device on 2026-11-13 with injected presses and screenshots, with
  copies (number 900) of the user's two pictures: the tile and its line, the grid with the
  copies first, the large view of a Game Boy picture, Y with its question and A deleting it,
  then Super Mario Bros.' details showing 2, its gallery with only its two, deleting one
  there and the row showing 1 on the way back. The user's pictures were left as they were.
  Not tried: more than a page of pictures, a broken file, the lid, the German wording on the
  device (checked in `--preview-gallery` with `LAUNCHER_PREVIEW_LANGUAGE=de`). The previews
  are `--preview-gallery` and `--preview-gallery-full`.

- Button remapping and turbo. CONTROLS in the in-game menu (`controls.rs`,
  `canvas::controls_screen`) opens a page with a row for each of A, B, X, Y, L1, R1, L2, R2,
  SELECT and START. Left and Right choose what the button presses in the game: any of those,
  a d-pad direction or NONE. A turns turbo on or off for that button, so any number of buttons can
  have it. TURBO SPEED is FAST, NORMAL or SLOW, for the whole layout: 2, 3 or 5 frames pressed
  and as many released, so about 15, 10 or 6 presses a second at 60 fps. One frame each way
  isn't offered, as a game that reads the buttons every other frame could see no change. The
  d-pad itself and Menu can't be changed, so no layout leaves a game unplayable or the menu
  out of reach. The button names are the device's, whatever a console calls them, and a target
  the console doesn't have does nothing.

  SET FOR says whom changes are for. THIS CONSOLE keeps a line like
  `controls.gb=normal,x:right*,r2:none` in `settings.txt`, holding the speed and what differs
  from the default (`*` is turbo), and the default has no line. THIS GAME copies what the game
  had been using into `Tetris.gb.srm.controls` beside its saves, which then wins over the
  console's. It follows the save's name through zipping, and DELETE GAME AND SAVES removes it,
  while it counts neither as a save nor as last played. Going back to THIS CONSOLE removes the
  file, though the page remembers it until it closes, in case that was a slip. RESET THESE
  CONTROLS empties the layout shown. RESET SETTINGS removes console lines and all games'
  files. A line or file with anything unknown is ignored as a whole. Changes are saved and
  in use as they're made.

  `Pad::update` works out what the game sees once per frame, in `Core::run_frame`, and
  `input_state` only reads that, so all a core's reads in a frame agree and fast-forward's
  extra frames count as frames. A turbo button's cycle starts over with each press, so a quick tap
  always reaches the game. `Host::controls` stays the real buttons, for Menu + Start and the
  menu. R2 fast-forwards only while its row is unchanged; a changed R2, including R2 with
  turbo, is for the game alone, which the page notes on that row. A PlayStation game with R2
  unchanged still sees it and fast-forwards, as before. There is no global default, and
  nothing in the launcher's Settings: controls are set from inside a game.

  The menu gained a row, so `PAUSE_ROW` went from 30 to 27 lines to stay on its panel.
  `--preview-controls` renders the page. Claude tested on the device on 2026-11-09 with Tetris and a
  save in `/tmp`, by injected presses and the framebuffer: X set to START passed the title
  screen, and Start still worked. On the level grid, holding X as RIGHT for about 0.6 s moved one cell, and
  with turbo at NORMAL six. Held R2 showed the fast-forward sign, and set to SELECT didn't.
  THIS GAME wrote its file and left the console's line alone, a restart opened on it, THIS
  CONSOLE removed the file, and RESET removed the line, bringing fast-forward back. German was
  checked in the previews. `settings.txt` was put back afterwards. How turbo feels in a real
  game, other consoles' cores and Menu + Start with a layout (`press.py` can't hold both) are
  not checked

- Core options. Each core's own settings, such as FCEUmm's sprite limit and palette or
  mGBA's Game Boy model, are on a CORE OPTIONS page in the in-game menu and under Settings >
  CORES (`core_options.rs`, `core_options_screen.rs`, `screens::option_rows`). Every option
  the core declares is listed under the core's own categories, with its own names, choices
  and help text, which are in English whatever the language. Left and Right go through the
  choices, A opens a category or steps a choice, B goes back, and RESET ALL TO DEFAULTS
  empties the choices shown. The selected option's help text shows under the rows, cut with
  `..` after three lines; long names and choices are cut the same way.

  The frontend now answers `GET_CORE_OPTIONS_VERSION` with 2 and takes
  `SET_CORE_OPTIONS_V2` and `_V2_INTL` (reading `us`; the frontend reports no language, so
  `local` is empty). Cores built with `libretro_core_options.h` send only `_V2_INTL` and
  don't fall back if it is refused. `SET_VARIABLES` is read too, for Tamalib, which
  declares nothing. Since 2026-11-13 the first interface, `SET_CORE_OPTIONS` and
  `SET_CORE_OPTIONS_INTL` (definitions without categories), is read too, for PokeMini and
  Beetle VB, which send it after a version of 2 and fall back to nothing when it is refused.
  Answering their options showed that Beetle VB calls its log callback unchecked, so the
  frontend now answers `GET_LOG_INTERFACE` with `launcher_core_log` from `core_log.c`, in C
  as it takes variable arguments. It prints warnings and errors to the launcher's log, and
  stops after 300. Handy's and PrBoom's log patches are older and still harmless. Stella
  logs one `File open/read error` as it starts, and the game runs. `SET_CORE_OPTIONS_DISPLAY` hides options the core says don't apply, and
  the `UPDATE_DISPLAY_CALLBACK` is called after each change, so the page follows. Options
  with one choice, and mGBA's `mgba_interframe_blending`, which IN GAME > LCD GHOSTING
  sets, are left out. `FRONTEND_DEFAULTS` (SGB borders off, colour correction Auto, FCEUmm's
  Smooth FBX palette, no second PlayStation memory card) are the defaults the page shows
  and RESET goes back to.

  What a core gets for an option: its ghosting setting, then the choice if the core still
  offers that value, then the frontend's default, then the core's declared default for
  cores with `declared_defaults`. Cores without it (mGBA, FCEUmm, Snes9x, Tamalib) still
  get nothing for options nobody chose, as before, except an option chosen earlier in the
  same run, so going back to the default reaches the core. Values the core is given are
  kept for the whole run (`Host::answers`), as a core may hold the pointer.

  SET FOR works as on CONTROLS: THIS CONSOLE keeps `key=value` lines in
  `/mnt/data/core-options/<console>.txt`, only for values that differ from the default, and
  removes the file when none are left. THIS GAME copies what the game had been using into
  `Tetris.gb.srm.options` beside its saves, where an empty file means every default. Going
  back to THIS CONSOLE removes it. DELETE GAME AND SAVES removes the game's file. It counts
  neither as a save nor as last played. RESET SETTINGS removes the folder and every game's
  file. Consoles that share a core (Game Boy, Color and Advance on mGBA) keep their own
  choices. Changes are saved and handed to the core as they are made. The core reads them
  through `GET_VARIABLE_UPDATE` on the next frame, but some only apply on the next start,
  which the page says and which mGBA marks with (RESTART) in the name. Some options do
  nothing here, such as FCEUmm's aspect ratio, as the frontend sets the shape itself.

  Settings > CORES lists the 16 consoles, and each opens its core's options for the
  console, without SET FOR. The launcher never loads a core: `launcher --core-options CORE`
  calls only `retro_set_environment`, where every core here declares its options, and
  prints them as tab-separated lines with what the core hid. The launcher keeps the result
  while it runs. FCEUmm declares its options again on loading a game, with its dipswitches.
  The tile is labelled CORES, as CORE OPTIONS is too long for a tile; it is the ninth, on a
  row of its own. The in-game menu gained a row, so `PAUSE_ROW` went from 27 to 25 lines.
  The in-game page uses a wider panel than the menu (600 lines), as names come from the
  cores. `--preview-core-options` renders it with sample options.

  Claude tested on the device on 2026-11-09. `--core-options` counts: FCEUmm 44 options in
  6 categories, Genesis Plus GX 62 in 6, Beetle PCE Fast 32 in 5, PCSX ReARMed 60 in 7,
  Snes9x 43 in 3, mGBA 17 in 5, Beetle Cygne 16, Handy 7, Beetle NeoPop 1, Tamalib none.
  Tetris was run with its save in `/tmp`. The page showed mGBA's categories. Changing the
  palette to DMG Green coloured the game on resuming and wrote `gb.txt`, and a restart
  started green. THIS GAME with RESET gave an empty game file and grayscale, while
  `gb.txt` stayed. THIS CONSOLE removed the game's file and brought green back, and RESET
  removed `gb.txt`. Super Mario Bros. showed Smooth FBX as its palette, and NES Classic took
  effect on resuming. Settings > CORES listed the consoles and NES's categories. Everything
  was put back, and the folder removed. Not checked: German wording on the device (the
  previews were), a core's hidden options changing during a game, PCSX ReARMed and Genesis
  Plus GX in a game, and options that only apply on a restart for each core

- Cheats. CHEATS in the in-game menu lists the codes of the game's cheat file, each ON or
  OFF (`cheats.rs`, `canvas::cheats_screen`). A, Left or Right turns the selected code on
  or off, L1 and R1 move 12 rows, TURN ALL OFF does that, and DOWNLOAD CHEATS fetches the
  game's file. Under the rows is the selected code's whole description, which its row may
  cut; with no codes, where to copy a file. The page uses the CORE OPTIONS panel and its
  row drawing, now shared through `screens::RowList`. Both wide pages' titles moved up 8
  lines, so the arrow for rows above the page no longer overlaps them.

  Files are RetroArch `.cht` files, in `/mnt/data/cheats/<console key>/` on TETSU-DATA
  (`cheats/gb`, `cheats/snes`, ...). A game's file is the one named after it (its file
  name without the extension, so a ZIP's name), or else the one whose title matches,
  compared on letters and digits only, with the most tags in common, as box art is
  chosen. Quoted values may span lines, as some descriptions in libretro-database do;
  they are shown on one line. Codes without a code, and RetroArch's own memory-search
  cheats (`handler = 1`, such as the `(Rumbles)` files), are left out, as this frontend has
  no cheat engine of its own: the core applies each code itself through
  `retro_cheat_set`, in whatever formats it reads (Game Genie, Pro Action Replay, GameShark,
  Code Breaker or raw `address:value`). The CHEATS item shows only for consoles whose core
  does that (`System::cheats`): not the Lynx, Neo Geo Pocket or Tamagotchi, whose cores
  ignore codes.

  Turning a code on or off calls `retro_cheat_reset` and then gives the core the codes that
  are on, numbered from 0, as RetroArch does: mGBA takes every code it gets as on. The codes
  that are on are saved by their code in `Tetris.gb.srm.cheats` beside the saves, so a
  new download keeps those still in the file, and the file is removed when none are left.
  A start gives the core those codes again after the resume state is loaded, leaving out
  any no longer in the cheat file. The `enable` lines of a `.cht` file are ignored: codes
  only come on from the page. Like a game's controls, the file counts neither as a save
  nor as last played, DELETE GAME AND SAVES removes it, and RESET SETTINGS removes every
  game's list; the cheat files themselves stay.

  DOWNLOAD CHEATS runs on its own thread while the menu stays usable, and the row reads
  DOWNLOADING... meanwhile. It first asks raw.githubusercontent.com for
  `libretro-database/master/cht/<console folder>/<game>.cht`, which needs no listing, then
  GitHub's listing of the folder (`git/trees/master:cht/<folder>`, one API call, of the 60
  an hour allowed) for the closest name. A file without usable codes counts as none. It is
  saved under the game's own name, so it is found first from then on. The result shows in
  the accent colour: `37 CHEATS DOWNLOADED`, NO CHEATS FOUND FOR THIS GAME, or the box art
  download's messages, such as NO CONNECTION TO GITHUB. The folders hold 1,500 to 2,300
  files per console, named in several ways: No-Intro names, bare titles, and tags for the
  device, such as `(Game Genie)` or `(GameShark)`.

  mGBA's `retro_cheat_reset` freed its codes without undoing their changes, so a Game Genie
  or other ROM patch code, and a GBA hook code's breakpoint, stayed after being turned off.
  `package/libretro-mgba/0001-undo-cheats-on-cheat-reset.patch` disables, removes and frees
  each set instead. Snes9x and Genesis Plus GX restore the ROM themselves; FCEUmm's codes
  are read through its cheat list, which reset empties.

  Claude tested on the device on 2026-11-09, running Tetris with its save in `/tmp`. Real
  downloads from the PC: Super Mario Bros. (World) found by name (711 codes), Tetris (World)
  (Rev 1) (37), Pokemon Emerald and Crash Bandicoot by the closest name. On the device the
  menu showed CHEATS, the empty page its note, and a download with the Wi-Fi not passing
  traffic NO CONNECTION TO GITHUB; after a reboot it saved 37 codes and listed them. Game
  Genie "Glitched Graphics" scrambled the next screen. GameShark "No Piece Display Box"
  emptied the next-piece box, and "Always Get Square Pieces" dropped squares and showed a
  piece in the level select, which TURN ALL OFF ended. Both came back after quitting and
  starting again, with a code no longer in the file left out (`cheats: 1 on`).
  "Glitched Graphics" on from a fresh start scrambled the level select after the title
  screen. Turned off on the copyright screen, the unpatched core still scrambled it, and the
  patched core showed the game type screen as normal. Not checked: the other cores, a PlayStation code, a ZIP, the
  German wording on the device (the previews were)

- RetroAchievements (`achievements.rs`), through rcheevos v12.5.0, the library RetroArch
  uses, copied into `src/launcher/rcheevos/` (MIT; `README.md` there says what was left
  out and how to update) and compiled into the launcher by `build.rs` with the `cc` crate.
  Buildroot's `TARGET_CONFIGURE_OPTS` gives it the cross compiler. The FFI is written by
  hand, mirroring only the structs and fields read. `rcheevos/libretro/libretro.h` is the
  part of libretro.h that `rc_libretro.c` needs: the memory constants and the memory map.

  Settings > ACHIEVEMENTS (the tile says TROPHIES, as ACHIEVEMENTS is one character too
  long for it) has ACCOUNT and HARDCORE. A on ACCOUNT types the user name and then the
  password on the on-screen keyboard and logs in on a thread with `rc_client`'s password
  login. A failed login goes back to the password keyboard with the name and password
  still typed and the reason, such as WRONG NAME OR PASSWORD, above the field; B on an
  empty password goes back to the name, also still typed, and B on an empty name leaves.
  Only the name and the token the server returns are kept, in
  `/mnt/data/retroachievements.txt`, mode 600, outside `settings.txt`, so RESET SETTINGS
  keeps the account. Logged in, the row shows the name, and A asks PRESS A AGAIN TO LOG
  OUT, which removes the file. HARDCORE is a setting, off by default (`hardcore=on`).

  ACHIEVEMENTS, the group's first row, turns them off for every game (`achievements=off`),
  keeping the account. A game's details screen (Y in a list) has an ACHIEVEMENTS row, ON or
  OFF like a setting, which A, Left or Right switches, for games of consoles
  RetroAchievements covers (`Library::achievement_systems`),
  which leaves an empty `Tetris.gb.srm.achievements-off` beside the saves. Both apply from
  the game's next start: a game with achievements off starts no session, so it has no
  notices, no ACHIEVEMENTS item, and none of hardcore's rules. The file counts as a setting,
  not a save, DELETE GAME AND SAVES removes it, and RESET SETTINGS removes every game's.
  The details rows moved up 4 lines to fit a fourth.

  A game process with an account, on a console RetroAchievements covers (`console_id`; all
  but the Tamagotchi), starts a `Session` after `retro_load_game`: it logs in with the
  token and calls `rc_client_begin_identify_and_load_game` with the game's path, which
  rcheevos hashes itself: the file unpacked from a ZIP, an `.m3u`'s first disc, a `.cue`'s
  data track. rcheevos' own disc reader handles `.cue`/`.bin` and `.iso`, not `.chd`, so a
  CHD game is NOT RECOGNISED. Changing a disc of an `.m3u` game calls
  `rc_client_begin_identify_and_change_media` with the disc's path.

  Memory: `RETRO_ENVIRONMENT_SET_MEMORY_MAPS` (36) is answered, and the map is copied with
  its address space names, as a core's own may not outlive the call. `rc_libretro_memory_init`
  then lays the console's address space over the map, or over `retro_get_memory_data`'s
  regions for a core without one; it knows each console's layout. A new map lays the
  memory out again, as RetroArch does: mGBA gives its map only in its first `retro_run`
  (its deferred setup), and before that only its work RAM, so high RAM at $FF80 was
  missing and rcheevos disabled every Super Mario Land achievement at load (`Invalid
  address 00FF9F`); `a_memory_map_given_after_the_start_is_used` tests it. A core that
  shows no memory yet is asked again every 60 frames while the game loads. `rc_client_do_frame` runs
  after every `retro_run`, fast-forward's extra frames too, and `rc_client_idle` every
  100 ms while the menu is open.

  Requests: `rc_client` hands each request to `server_call`, which runs it on a thread with
  `ureq` (HTTPS only, 15 s to connect, 30 s in all) and the user agent
  `TetsuOS/0.1.0 (Linux) rcheevos/12.5.0`. The answer goes back through a channel, and
  `Session::pump`, called every frame, gives it to rcheevos on the game's thread, so
  rcheevos and every callback run on one thread. A request that never reached the server
  answers `RC_API_SERVER_RESPONSE_RETRYABLE_CLIENT_ERROR`, so rcheevos retries unlocks
  itself. Leaving the game waits up to 5 s for requests still on their way.

  Offline (2026-11-13, `achievements_offline.rs`): every request goes through
  `offline::answer`. The answers a game needs to start, the token login (`login2` with a
  token, never a password), the game's achievements (`achievementsets`, and `gameid` for
  a disc change) and the session with the user's unlocks (`startsession`), are kept as
  `/mnt/data/achievements/offline/<user>-<request>-<game hash>.json` whenever the server
  gives them. When the server can't be reached (no answer, or a 5xx), the kept answer is
  given to rcheevos instead, so a game played once online has its achievements offline,
  with OFFLINE - UNLOCKS WAIT FOR A CONNECTION after the loaded notice. A game never
  played online still fails with NO CONNECTION TO RETROACHIEVEMENTS.
  An unlock that can't reach the server is added to `waiting.txt` there (user, ID,
  hardcore, game hash, Unix time; no token) and rcheevos is told it went through, so it
  doesn't retry it; it is added to the kept session too, so the game starts with it
  unlocked next time offline, and an unlock that reaches the server is added there as well. The
  notice UNLOCKS WAIT FOR A CONNECTION shows once, for a connection lost during a game.
  Leaving a game (`Session::finish`, 2026-11-13) waits at most 0.5 s
  (`FINISH_TIMEOUT`), and only while an unlock is being sent (`offline::unsent`); those
  still unanswered then go to `waiting.txt` too (`keep_unsent`; one the server took
  meanwhile is answered `User already has` when sent). A login, `achievementsets`,
  `startsession` or ping still on its way is left behind, as the next game asks again.
  It used to wait up to 5 s for any request: with Wi-Fi hung (see Known issues in
  `CLAUDE.md`) each DNS lookup takes about 12 s to fail, so nearly every quit waited the
  full 5 s, while 0.36 s was measured from QUIT to the menu drawn with working Wi-Fi.
  With DNS pointed at an address that never answers and the game's achievements still
  pending, the game process now ends 0.17 s after the press.
  `sync` sends the waiting unlocks, rebuilt with rcheevos' own
  `rc_api_init_award_achievement_request` and the token of the account logged in, with
  `o=` the seconds since the unlock (signed into `v=`), so the server dates them when
  they happened: before each `startsession` (WAITING UNLOCKS SENT then shows), and every
  minute from a thread in the launcher's menus (`WAITING_RETRY`). Taken, or `User
  already has`, removes one; no connection, 401 or 403 stops and keeps the rest; any
  other refusal drops that one with a log line. `waiting.lock` (`flock`) keeps the
  launcher and a game from writing the file at once. Unlocks of another account wait
  until it logs in again; logging out removes the kept logins, which hold the token.
  Not offline: GAME COMPLETED and MASTERED (rcheevos takes them from the server's
  `AchievementsRemaining`), leaderboard entries (retried only while the game runs), and
  the player's score, which reads 0 after an unlock offline. An unlock dated by a wrong
  clock (no network time yet) is sent with that age. Tests:
  `a_game_played_before_unlocks_offline_and_sends_it_later` (a whole session online,
  offline with an unlock, offline again, online) and `achievements_offline`'s own.

  Notices show at the bottom of the screen, centred, for 4 to 6 s, through the
  framebuffer's sign, like the volume, and one after another: the game loaded (`3 OF 40
  ACHIEVEMENTS UNLOCKED - HARDCORE` over its title), an unlock with its badge and points,
  progress towards one (2 s, a newer one replacing any waiting, and only as it reaches
  25, 50 and 75 % of the way, as rcheevos reports every step: Super Mario Bros.' 180
  jumps showed a notice on each jump), GAME COMPLETED or GAME
  MASTERED, leaderboards started, failed and submitted (only in hardcore, where rcheevos
  runs them), a game the server doesn't know or that has no achievements, a failed login
  (WRONG NAME OR PASSWORD, LOG IN AGAIN IN SETTINGS for an expired token, NO CONNECTION TO
  RETROACHIEVEMENTS), and unlocks waiting for a connection and sent later. Leaderboard
  trackers and challenge indicators aren't shown.

  Each kind of notice has its own place: UNLOCKS (NEUE ERFOLGE), PROGRESS (FORTSCHRITT)
  and OTHER MESSAGES (SONSTIGES), everything else, from the loaded game to HARDCORE OFF
  FOR THIS GAME. Each is BOTTOM (the default) or TOP, 8 pixels from the edge either way,
  or HIDDEN, which leaves that kind out (`Notice::kind`). Hidden achievements still
  unlock, and the in-game menu's list still shows them. Settings > ACHIEVEMENTS has the
  three for every game (`notices_unlocks=`, `notices_progress=`, `notices_other=` in
  `settings.txt`). In a game, the in-game menu's ACHIEVEMENTS opens a list of LIST (the
  achievements page), then the same three for that game, each DEFAULT (STANDARD), which
  follows Settings, or a place of its own. The full Settings value didn't fit beside the
  German labels, so DEFAULT shows alone. A game's choices are kept as
  `Tetris.gb.srm.notices` beside its saves (`progress=hidden` lines, removed once all are
  DEFAULT), which count as game settings like `.achievements-off`: DELETE GAME AND SAVES
  and RESET SETTINGS remove them.

  Badges are downloaded once the game has loaded, on a thread, one after another, as
  `/mnt/data/achievements/<badge name>.png`, stopping at the first failure to connect. The
  page shows them at 32 pixels, averaged down from 64, and grey and darker while locked,
  made from the same file; a notice shows them at 64.

  ACHIEVEMENTS in the in-game menu, after OPTIONS, shows for a game with a session, and
  opens a list whose LIST is the achievements page. The page, on the CORE OPTIONS panel,
  has `2 OF 6 UNLOCKED - 6/101 POINTS - HARDCORE` and the achievements in rcheevos' groups (ALMOST THERE, LOCKED, UNLOCKED, ...), each with its
  badge, title and points, or its progress such as `73/100`; Up and Down move, wrapping,
  L1 and R1 (or Left and Right) a page, and the selected one's description is below.
  While loading, or for a game without achievements, it says so. The in-game menu's rows
  went from 23 to 22 lines, so the fifteenth item fits.

  The game's count is saved as `Tetris.gb.srm.achievements` beside its saves when it loads
  and after each unlock; a game's details screen shows ACHIEVEMENTS `3 OF 40` from it.
  It counts neither as a save nor as last played, and DELETE GAME AND SAVES removes it.

  Hardcore (`Session::restricts`): unlocks count as hardcore, the game starts from its save,
  not its resume point, LOAD STATE says HARDCORE ALLOWS NO LOADING, the CHEATS item and
  saved codes are left out, and R2 doesn't fast-forward. Saving states still works. The
  restrictions end once the game turns out to have no achievements or isn't recognised,
  but the resume point was already skipped. Core options that RetroArch disallows in
  hardcore (`rc_libretro_get_disallowed_settings`: a PAL region for FCEUmm, Snes9x, Genesis
  Plus GX and PCSX ReARMed, Game Genie for FCEUmm, Snes9x's layer and clipping options,
  PCSX ReARMed's lower clock) switch the game to softcore at start, or when changed on the
  CORE OPTIONS page, with HARDCORE OFF FOR THIS GAME: CORE OPTION and the option's name.
  Loading a state in softcore calls `rc_client_deserialize_progress_sized` with nothing,
  which restarts the achievements from the loaded state, so its memory jump isn't counted.

  Tests: `cargo test achievements` runs a whole session offline against canned server
  answers taken from rcheevos' own tests (a test-only `FAKE_SERVER`): password login and
  a refused one, token login, hashing the file, loading the game, reading a fake 2 KiB NES
  RAM through `rc_libretro`, an unlock with its notice, and the saved count. Previews:
  `--preview-achievements` and `--preview-notice`. `--render` also prints how much memory
  rcheevos finds at load and after the frames (`achievement memory at load: N bytes in M
  regions`); only mGBA's Game Boy games differ, 5 regions at load and 11 after.

  The server adds a WARNING: UNKNOWN EMULATOR achievement for this client, as it does for
  one it doesn't know by its user agent (`TetsuOS/0.1.0 (Linux) rcheevos/12.5.0`), and
  rcheevos unlocks it at once and leaves it out of counts. Being known takes registering
  the client with RetroAchievements; borrowing another emulator's user agent isn't an
  option.

  Claude tested on the device on 2026-11-09: the settings grid with the TROPHIES tile, the
  ACHIEVEMENTS group logged out with its notes, and the user name keyboard, left with B.
  `--render` of 300 frames, with saves in `/tmp`, found memory for every core with a game on
  the card, each the size of rcheevos' layout for the console: FCEUmm 65536 bytes in 8
  regions (from 14 descriptors), Genesis Plus GX 40960 for the Master System and Game Gear
  and 131072 for the Mega Drive (no memory map: `retro_get_memory_data`), Beetle PCE Fast
  8192, mGBA 212992 for the Game Boy and Color and 360448 for the Advance, Snes9x 657408,
  PCSX ReARMed 2098176. The user logged in on the device; unlocks in softcore work, and
  hardcore is refused while the server doesn't know the client. Super Mario Land loaded with all
  34 achievements disabled until the late memory map was used, then with 34 of 35 active
  (the other is the warning). Not checked: an unlock reaching the server, notices and the
  page with an unlock, hardcore, the Neo Geo Pocket, WonderSwan and Lynx cores (no
  games on the card), a ZIP, a multi-disc game, the German wording on the device (the
  previews were), and anything over a real connection to the server
- **Card layout for A/B updates** (2026-11-12): `genimage.cfg` has TETSU-BOOT (FAT,
  128 MiB at 1 MiB), root slot A (`TETSU-ROOT`, 1 GiB, partition 2, bootable flag), an
  empty 1 GiB root slot B (partition 3) and, added by `flash.sh`, TETSU-DATA at 4 GiB as
  partition 4, which uses up MBR's four. The kernel and device tree are in each root
  slot's `/boot` (`BR2_LINUX_KERNEL_INSTALL_TARGET`), with `extlinux.conf` in
  `/boot/extlinux/` (installed by `post-build.sh`). U-Boot's bootstd scans only the
  partitions with the bootable flag when any has it, and reads `extlinux.conf` from
  `/` or `/boot/`, so the flag picks the slot; `extlinux.conf` paths are absolute
  (`/boot/Image`), which U-Boot's extlinux bootmeth allows. TETSU-BOOT keeps
  `README.txt`, `bootlog.txt` and `logs/`. Slot B has no filesystem and isn't in
  `sdcard.img` (genimage stops the file at slot A's end), so flashing takes as long as
  before. For updates later: U-Boot 2026.10 has `CONFIG_BOOTMETH_RAUC` (off), which keeps
  `BOOT_ORDER` and tries per slot in its environment (`CONFIG_ENV_IS_IN_FAT`, on
  TETSU-BOOT) and runs a `boot.scr` from each slot's boot partition, which can be the
  root partition itself (`CONFIG_BOOTMETH_RAUC_PARTITIONS="2,2 3,3"`); an update writing
  slot B must change `root=PARTUUID=54455453-02` in its `extlinux.conf` or script to
  `-03`. Checked in the image: the partition table, `/boot` in the root filesystem, the
  FAT's contents, and `data-partition.sh` on an 8 GiB card image (TETSU-DATA created as
  partition 4, then kept on a second run). On the device (2026-11-13), after the user
  flashed over the old layout: it boots from slot A (`root=PARTUUID=54455453-02`),
  TETSU-DATA mounts as `mmcblk0p4` with its games and settings kept, and the kernel
  starts 2.59 to 2.66 s after power on, as before (2.53); the launcher drew at 6.58 and
  6.60 s with the jingle. BOOT MESSAGES TEXT and back gave the original file byte for
  byte, and a reboot with TEXT had `console=tty1 loglevel=7` on the kernel's command
  line, so U-Boot reads the file the device wrote. SAVE LOGS TO CARD and `bootlog.txt`
  reach TETSU-BOOT. `push.sh boot` copied, verified, rebooted and put LOGO back.
- **TETSU-DATA on the first start** (2026-11-14): a card written with an image writer
  (balenaEtcher, Raspberry Pi Imager, `dd`) has only the image's partitions, so `S15data`,
  finding no TETSU-DATA, runs `/usr/sbin/data-partition`. It only acts on TF1's card with no
  fourth partition, at least 1 GiB past 4 GiB, partitions 1 to 3 ending before 4 GiB and the
  MBR's fourth entry empty. It writes that entry with `dd` (type `0x0c`, 4 GiB to the card's
  end, as `scripts/data-partition.sh` does) and tells the kernel with util-linux's `addpart`,
  as BusyBox's `partprobe` re-reads the whole table, which fails while root is mounted.
  A FAT32 file system labelled TETSU-DATA found there is kept (a card that had Tetsu OS and
  was written again); anything else is erased (its first MiB) and formatted with dosfstools'
  `mkfs.vfat -F 32 -n TETSU-DATA`, as writing the image meant erasing the card. Each step
  goes to the kernel log, so `bootlog.txt` shows it. The image gained `mkfs.fat` (59 KiB),
  `addpart` (30 KiB) and libblkid, libuuid and libsmartcols; `blkid` stays BusyBox's.
  Checked on the device with a sparse 5.5 GiB loop disk holding the card's MBR (`losetup
  -P`): an empty area formatted in 0.26 s, a second run left the fourth partition alone, the
  entry removed again kept TETSU-DATA with a file on it, and a FAT labelled OTHER was
  formatted; a 64 GiB loop disk gave a 60.0 GiB TETSU-DATA. On the real card, with its fourth
  MBR entry cleared and a reboot: `added /dev/mmcblk0p4 with the TETSU-DATA file system
  already there` 1.15 s into the kernel, mounted with the games and settings, the launcher
  drawn at 4.67 s after power on against 4.43 s on the next, normal, boot. Not done on the
  real card: formatting, as it would erase the device's games. Its time is estimated from
  the FATs, 15 MB for a 60 GB card, which the card writes in 0.79 s.
- **The boot card identified again** (2026-11-15): the v0.1 release card stopped 3 times
  in its first 4 cold starts at "Waiting for root device PARTUUID=54455453-02", and a
  freshly wiped and written SanDisk Ultra did on most cold first starts. The screen showed
  why: `mmc0: new SD card at address d555` and `mmcblk0: mmc0:d555 SD032 30.6 MiB` with no
  partitions, against `new high speed SDHC card at address aaaa`, `SD32G 29.7 GiB` on a
  good start. SanDisk cards give `aaaa` as their first address after power up and `d555`
  as the second, so the card was still in U-Boot's session: Linux's reset hadn't taken.
  TF1's supply is the always-on `vcc-io`, so Linux can't power cycle it. Taking the card
  out and putting it back, which does, made the boot carry on every time. Ruled out: the
  release build, Etcher, the data partition, U-Boot's environment file (written into the
  image beforehand, it still failed), the battery, a busy card (a kernel waiting on DATA0
  found it never busy), and RAM to keep the log (it doesn't survive a reset here). A 2 s
  `post-power-on-delay-ms` booted once, but costs every start. Patch 0019 instead makes
  `wait_for_root()` identify the SD memory cards again when the root device is missing 1 s
  after the drivers are done, then every 2 s, 3 times at most: each card is dropped, its
  host powered down and up and rescanned. A good start finds its root before it ever
  waits (no "Waiting for root device" line), so it is unchanged: root mounted at 0.537 s,
  launcher at 6.65 s. Checked by Claude on the device with `mmc_reidentify_test`, which
  forces one round: the card was removed at 0.493 s, found again at 0.608 s with its four
  partitions, the root mounted at 0.695 s and the launcher drew at 6.99 s. Not yet seen:
  a real failing start recovering by itself. The user saw it only with SanDisk Ultra
  cards, never with a 64 GB SanDisk Extreme, and a reset always starts the device, so it
  is left as it is (2026-11-16), with a line in the release notes
- **Updates over Wi-Fi** (2026-11-13): Settings > SYSTEM > SYSTEM UPDATE (`update/`)
  installs a new root filesystem, which holds the kernel too, into the root slot that
  isn't running, and keeps the running one to go back to. U-Boot is not updated this way.
  - **Making one:** `scripts/make-update.sh` gzips `output/images/rootfs.ext4` (62 MB for
    the 1 GiB image; zstd would be 50 MB but needs another crate, and gzip is in `flate2`
    already) into `output/images/update/` with `manifest.txt` (`format`, `device` from
    the device tree, `build`, `revision`, `file`, `file_size`, `image_size`,
    `image_sha256`) and `manifest.sig`, the manifest's Ed25519 signature in hex.
    `post-build.sh` adds `BUILD_ID=YYYYMMDD-HHMMSS` (UTC) to `os-release`, the same as the
    manifest's `build`; the script checks with `debugfs` that the image is that build. The
    private key is outside the project (`~/.config/tetsu/update-key.pem`, made once with
    `make-update.sh --new-key`, or `$TETSU_UPDATE_KEY`); its public half is
    `src/launcher/update-key.pub`, compiled in, so the launcher must be built after the
    key exists. `scripts/serve-update.sh` serves the folder on port 8035 with Python;
    over the cable that is `http://10.35.35.2:8035`.
  - **The screen:** rows for this system's build, revision, root slot and the source, and
    two lines of status. A looks for an update (`manifest.txt`, `manifest.sig` next to
    it), which must have the project's signature, this device's `compatible` and a newer
    `build` than the running `BUILD_ID` (a system without one is older than any). A again
    installs it, unless the battery is under 30 % and not charging. X types the source on
    the keyboard (`update_source=` in the settings; empty brings back the one built in
    with `TETSU_UPDATE_SOURCE`, none so far). Plain HTTP is allowed, since the signature
    is what is trusted; redirects are followed, so GitHub release assets would work.
    During an install a bar shows the download, then the read-back; B twice stops it, and
    the lid only turns the screen off. At the end A reboots, B leaves it for the next
    start. SAVE LOGS TO CARD moved from SYSTEM's rows to X on SYSTEM INFO, to make room.
  - **Installing** (`install.rs`): the boot order first puts the running slot first and
    the other at 0 tries, so a half-written slot is never booted, even after a power cut.
    The download is unpacked and written to the other slot's partition (`/dev/mmcblk0p3`
    or `p2`, found from the root device's disk) in 1 MiB chunks, synced every 64 MiB,
    with its SHA-256 taken; then the page cache is dropped (`POSIX_FADV_DONTNEED`) and the
    partition read back and hashed again. Both must match the manifest. The slot is then
    mounted to check `boot/boot.scr`, `boot/Image` and `extlinux.conf`, and its
    `extlinux.conf` gets `root=PARTUUID=54455453-03` (or `-02`) and the running system's
    BOOT MESSAGES. Last, the boot order puts the new slot first with 3 tries and the
    running one behind it, and `/mnt/data/update-pending.txt` notes the slot and build.
  - **Booting and falling back:** `uboot.fragment` turns on U-Boot's RAUC bootmeth
    (`BOOT_ORDER`, `BOOT_A_LEFT`, `BOOT_B_LEFT`, 3 tries, slot A partition 2 and B 3,
    each its own boot partition) and `bootflow scan; run distro_bootcmd` as the boot
    command. Without `CONFIG_BOOTSTD_FULL`, `bootflow scan` refuses flags and a device
    ("Flags not supported") and returns at once, which a first try with `-bG mmc0` did,
    so the old script booted and nothing was saved; plain, it boots the first bootflow,
    with the devices in `boot_targets`' order (mmc0 first). On the whole card the RAUC
    bootmeth comes before the extlinux bootmeth reaches the partitions. U-Boot takes a try off the first slot in
    `BOOT_ORDER` with any left, saves the environment and runs that slot's
    `/boot/boot.scr` (`boot.cmd`, made by `mkimage` from `host-uboot-tools` in
    `post-build.sh`), which `sysboot`s the slot's own `extlinux.conf`, so BOOT MESSAGES
    still works per slot. A slot whose script fails falls through to the extlinux
    bootmeth, which boots the bootable partition, slot A. At the launcher's start
    (`slots::mark_good_in_background`, once per boot through `/run/launcher-slot-marked`,
    in the charging screen too) the running slot goes first with its 3 tries back, so
    only boots that never reach the launcher count; after 3 of them U-Boot boots the
    other slot. `extlinux.conf` now has `panic=10` (was `panic=0`, which waits forever),
    so a kernel that panics restarts and counts as a failed try; a hang without a panic
    needs the power button held. If `update-pending.txt` names another slot than the
    running one, the update failed: SYSTEM UPDATE then says `<build> DID NOT START, THIS
    SYSTEM WAS KEPT`, and otherwise `UPDATED TO <build>`.
  - **U-Boot's environment** (`boot_env.rs`) is in two copies on TETSU-BOOT,
    `uboot.env` and `uboot-redund.env` (`CONFIG_ENV_REDUNDANT`), 64 KiB each: CRC32,
    a counter byte, then sorted `name=value` entries ending in NUL. The copy with a valid
    CRC and the higher counter (0 after 255) is current, and a save writes the other one,
    so a power cut mid-save leaves the current copy. Linux reads and writes them the way
    U-Boot does, changing only `BOOT_*` and keeping every other entry. The default
    `CONFIG_ENV_FAT_DEVICE_AND_PART=":auto"` meant the bootable partition, the ext4 root,
    where saving failed, so nothing was ever saved before; it is now `":1"`. U-Boot saves
    its whole environment, boot command included, and a saved boot command would outlive
    a later U-Boot's, so `push.sh uboot` removes both files when running slot A.
    A U-Boot without A/B never writes `BOOT_ORDER`, and then an install stops before
    writing anything: THIS CARD NEEDS ONE FLASH FOR UPDATES.
  - **Tested on the device** (2026-11-13), over the USB cable from `serve-update.sh`
    (the PC's firewall lets the device reach it there):
    - With the old U-Boot, the new system booted as before; the launcher logged that
      U-Boot keeps no boot order, found and verified the signed update, and its install
      stopped with THIS CARD NEEDS ONE FLASH FOR UPDATES before writing.
    - With the new U-Boot (`push.sh uboot`), U-Boot saved `BOOT_A_LEFT=2` in
      `uboot-redund.env` and the launcher wrote `uboot.env` with 3 back and every other
      entry kept. The launcher drew 6.73 s after power on, against 6.60 before.
    - Installing a newer build into slot B took about 1 min 45 s for the 62 MB download,
      writing 1 GiB and reading it back; the copies showed B at 0 tries while writing,
      then `BOOT_ORDER=B A`. A restarted into slot B (`root=PARTUUID=54455453-03`, the
      new `BUILD_ID`), marked good, `Updated` logged, drawn at 6.61 s.
    - Fallback: from B, slot A's `extlinux.conf` got `init=/sbin/tetsu-broken` and the
      order `A B` with 3 tries and a pending note for A, as an install leaves them. A's
      kernel panicked and restarted three times, and B was back after 59 s with
      `BOOT_A_LEFT=0`; the screen said `<build> DID NOT START, THIS SYSTEM WAS KEPT`.
      A's `extlinux.conf` was put back afterwards, so the card holds the newer build in B,
      running, and the older one in A.
    - Not tested: a download over Wi-Fi rather than the cable (the same code), the lid
      during an install, B to stop one, a low battery, typing a source on the keyboard,
      a power cut while writing, a U-Boot environment save cut short, and the German
      wording on the device.
  - **`push.sh` and slot B:** `push.sh boot` copies `boot.scr` too, and on slot B points
    the copied `extlinux.conf` at partition 3. `push.sh app` and `rootfs` write the
    running slot, whichever it is. Flashing writes slot A and a fresh TETSU-BOOT, so the
    card starts again from A.
- **Build setup** (2026-11-12): the Buildroot configuration is `configs/common.config`
  (what every device shares: system, packages, cores, ccache) followed by
  `board/<board>/buildroot.config` (CPU, toolchain, kernel, bootloaders, GPU and Wi-Fi
  firmware, the board's paths); `make config` concatenates them into
  `output/tetsu_defconfig` and runs `defconfig` with it, and `BOARD=` picks the board
  (only `rg35xxsp`). The cross toolchain is built once by `make toolchain` (also run by
  `make` when missing) in `output-toolchain/` from `configs/toolchain_a53_defconfig`
  (GCC 15, binutils 2.45, glibc, headers 7.1), and boards use its `host/` as a
  preinstalled external toolchain, so `make clean` keeps it. glibc has no RPC, so
  `BR2_TOOLCHAIN_EXTERNAL_INET_RPC` is off. `ccache/` holds the compiler cache
  (`BR2_CCACHE`); Rust doesn't use it. Compared with the old single defconfig, the
  merged configuration differed only in toolchain and ccache options. On this PC (16
  threads) the toolchain took 11 minutes and the clean image build after it 80, with an
  empty cache (1 GiB filled); `output-toolchain/` takes 7.5 GiB, mostly its build folder.
- **Device profile** (2026-11-12): `device/profile.rs` picks a `Profile` from
  `/proc/device-tree/compatible` (the device's own name, not the chip's): its name, and
  the battery, USB power and backlight folders that `power.rs`, `charging.rs`,
  `settings.rs` and `sleep.rs` used as constants. An unknown device logs a warning and
  gets the RG35XX SP's; a PC, with no device tree, too. The log has `device:` after
  `launcher started`, and SAVE LOGS TO CARD a `device` line. Features some devices lack
  (lid, vibration motor, second slot, Wi-Fi, an analog stick) join it with the first such
  device, as nothing would read them yet.
- **One image for the H700 family** (2026-11-19): `board/rg35xxsp/` became `board/h700/`,
  and the image carries the device trees of the RG35XX SP, Plus, H and 2024 in each slot's
  `/boot/allwinner/` (`BR2_LINUX_KERNEL_DTB_KEEP_DIRNAME`, about 33 KB each). Nothing before
  Linux tells these devices apart, so `device.txt` on TETSU-BOOT names the device
  (`device=rg35xx-plus`), which `boot.scr` imports and turns into `fdtfile` for
  `extlinux.conf`'s `FDTDIR /boot`. A missing file, or a name without a device tree, boots
  the SP's, so every card from before is unchanged. The choice lives outside the slots, so
  updates and the fall back to the other slot keep it. `flash.sh --device` writes it;
  TETSU-BOOT's `README.txt` explains it for image writers. U-Boot only reads the chosen
  device tree; reading `device.txt` and checking the device tree is there cost about
  0.05 s: the kernel started 2.78 to 2.82 s after power on in three reboots, against 2.74 s
  with the old script and its fixed `FDT` line on the same build (launcher drawn at 6.96 to
  7.08 s, with the jingle). U-Boot's old hush parser stops at a `'` even inside double
  quotes, and a script that stops leaves the boot to the extlinux bootmeth, which boots
  slot A with whatever `fdtfile` was set: a first version of the script did that, so every
  boot skipped the A/B order, and an unknown name hung. `post-build.sh` now refuses a
  `boot.cmd` with a `'` outside comments. The script was checked in U-Boot's sandbox
  (same source and parser) with no file, CRLF and LF lines, an unknown, empty and garbled
  name. Patches 0006 (the RAM voltage range) and 0007
  (USB device mode) moved to the RG35XX 2024 device tree and 0013 (Wi-Fi keep-power) to the
  Plus's, which the others include; the rest stay the SP's. Launcher profiles for the
  Plus, H (3300 mAh) and 2024 (2600 mAh) share the SP's power and backlight paths. The
  manifest gained `devices`, every device tree's `compatible`; `device` stays the SP's for
  v0.1 launchers, which only read that. An install also checks the new slot has this
  device's device tree. Not yet handled: the 2024 has no Wi-Fi or Bluetooth, the H has
  sticks, and the Plus, H and 2024 have no lid; the X/Y swap (0008), TF2 (0018), vibration
  (0015) and panel ID (0020) are SP-only until checked on the others. Only the SP has
  started this image. Checked on the SP (LPDDR3): no `device.txt`, `device=rg35xx-sp`
  with CRLF and an unknown name boot the SP's device tree; `device=rg35xx-plus` with CRLF
  booted the Plus's on the SP, with the Plus profile in the log, USB networking (0007 from
  the 2024's tree), `wlan0` and `keep-power-in-suspend` on mmc1 (0013), vdd-dram kept at
  1.2 V (0006), and no lid or vibrator, as the Plus has none. Not checked: a real Plus, H
  or 2024, the screen with the Plus's tree, `flash.sh --device`, and an update installed
  through the new `devices` check.

- **Save backup** (2026-11-13): the saves kept off the card, so a dead card loses nothing.
  A backup is the same set both ways (`backup::FOLDERS` and `FILES`): `saves` (saves,
  states, resume points, play time, per-game settings), `core-options`, `cheats`,
  `screenshots`, and `settings.txt`, `favorites.txt`, `collections.txt`,
  `hidden-games.txt`, `recent.txt` and `play-log.txt`.
  Hidden files and the `.tmp` files of a write under way are left out, and so are games,
  covers, `wpa_supplicant.conf` and `retroachievements.txt` (passwords and a token).
  - **Syncthing**, Settings > DATA > SAVE BACKUP (`backup/`, `save_backup=`, off by
    default). The official static arm64 build of Syncthing 2.1.7 (`package/syncthing`,
    32 MB in `/usr/bin`) runs from `/usr/sbin/syncthing-service`, with its keys, config and
    database in `/mnt/data/.syncthing`, so the device ID outlives updates. Its web page and
    REST API listen on 127.0.0.1:8384 only; the launcher reads the API key from
    `config.xml`. Each start sets it up (`syncthing::Api::configure`): one folder,
    `tetsu-data`, which is TETSU-DATA itself with `.stignore` taking in only the backup,
    send-only, `ignorePerms` and a 2 s time window for FAT32; no `default` folder, usage
    reports or upgrades; the device named by its hostname. The launcher starts it after
    the first frame while SAVE BACKUP is on, holds it with SIGSTOP during each game (it
    then takes no time from frames) and asks it to look for changes once the game ends, and
    stops it for USB STORAGE, which unmounts TETSU-DATA. `S44syncthing` stops it at power
    off, before `S15data`.
  - **Pairing:** the screen shows this device's ID at the bottom. Adding it in Syncthing on
    a PC, phone or server makes that device ask to pair, and it shows here as WANTS TO PAIR.
    A, then A again, pairs and shares the folder with it, which the other device then
    accepts (FOLDER NOT ACCEPTED until then; then SENDING n% and UP TO DATE). X dismisses a
    request; X twice unpairs. BACK UP NOW looks for changes at once. On the other device,
    file versioning on the folder keeps what the handheld deletes (a deleted game's saves),
    as send-only passes deletions on.
  - **Restoring** is A, then X, on a device asking to pair, as on a new card after the old
    one died: the new card has a new ID, which is added on the PC to the same folder. The
    card's own backup files are moved to `.restore-set-aside` and the folder made again,
    receive-only, with that device, so Syncthing knows nothing of the new card's files
    (whose newer times would otherwise win over the backup's). Once the device has accepted
    the folder and everything has come, local changes are reverted and the folder turns
    send-only; the launcher then starts again (it `exec`s itself), as its settings and lists
    changed under it. A device with no files, 15 minutes without progress, or B twice
    stop it: the device is unpaired again and the files set aside are put back. A restore a
    power cut stopped goes on when the screen opens (the folder is still receive-only).
  - **WI-FI TRANSFER's page** has a Backup card: Download backup streams the set as one ZIP
    (`tetsu-backup-<hostname>-<date>.zip`, `GET /api/backup`, written into a pipe as it is
    sent, PNGs stored, the rest deflated, file times kept in UTC), and Restore from a backup
    takes such a ZIP (`PUT /api/restore`): received into a hidden file on TETSU-DATA, then
    each backup file in it replaces the one on the card, with its time; other entries,
    paths outside the set and files over 256 MB are skipped, and files the ZIP lacks stay.
    The screen says BACKUP RESTORED, and leaving it starts the launcher again.
  - Tested on the PC: `cargo test` covers the set, the ZIP both ways, refused entries,
    setting files aside and back, the ignore lines, the API key, reading devices and
    completion, the menu and the screen's lines; `--preview-backup` renders the screen and
    `--preview-settings-grid` the grid with DATA.
  - Checked by Claude on the device (2026-11-13), with a second Syncthing on the handheld
    itself (`/tmp`, API on 8386 through an SSH tunnel) standing in for the PC: turning it
    on set up the folder and `.stignore` and showed the ID; the stand-in's request showed
    as WANTS TO PAIR, A A paired, and once the folder was accepted it held exactly the set
    (69 files, 12.9 MB, no games) and the screen said UP TO DATE. Syncthing used about
    80 MB of RAM at most for the whole system. `pause` stopped both processes (state T); a
    save written meanwhile went out within 20 s of `resume`. X X unpaired, the stand-in
    asked again, and A X restored: the card's files were set aside, a file only on the
    stand-in arrived, settings and saves came back byte for byte with their times, the
    launcher started again, and the folder was send-only and paired after. The page's
    ZIP (72 files, 3.3 MB, 1.6 s over USB) unpacked without errors; a non-ZIP was
    refused (415), a restore put 73 files in place and left out a `roms` entry, and B
    restarted the launcher. Turning it off stopped Syncthing within 4 s. Changing a
    folder blocks Syncthing's API until its rescan ends (over 10 s on the device), so calls
    wait up to a minute and the screen keeps its last state for up to 5 failed readings.
    Not checked: a real PC or phone, Wi-Fi rather than the USB link, a real new card,
    USB STORAGE with Syncthing running, a game's end sending its save, the lid during a
    restore, global discovery and relays (the image has no CA certificates, so the relay
    pool's HTTPS may fail; on a home network local discovery is enough), and the German
    wording on the device.

## Hardware facts in full

- **TF2 is `mmc2`, with its own 3.3 V switch on PE4,** patch 0018 (values from ROCKNIX's
  0126 and 0159). With that supply off, the card is powered back through its pulled-up
  I/O pins, so it stays on, as the vendor firmware leaves it. The MMC hosts probe
  asynchronously, so without aliases the card numbers could change; root is found by
  PARTUUID either way.
- **The vibration motor is on PH2, PWM channel 2,** patch 0015. It spins from 20 % duty.
  Its input device comes before the others, so `/dev/input/event*` numbers aren't fixed.

- **Don't trust the model name for hardware details.** This unit has LPDDR3 at
  1.2V, while mainline U-Boot's config and the kernel device tree assume LPDDR4
  at 1.1V, and another documented SP has LPDDR4. Read the working boot0 instead.
  It starts 8 KiB into the card. At these offsets: `0x38` clock, `0x3c` type
  (7 LPDDR3, 8 LPDDR4), `0x40` DX_ODT, `0x44` DX_DRI, `0x48` CA_DRI, `0x4c` ODT_EN,
  `0xa0` TPR6, `0xa4` TPR10, `0xa8` TPR11, `0xac` TPR12.
- **The SPL detects the RAM type** (U-Boot patch 0001). It tries LPDDR4 at 1.1V first,
  counting it only if it trains on the full 32-bit bus and passes the size check, then
  LPDDR3 at 1.2V, so LPDDR4 chips never get 1.2V. The settings for both are in
  `board/h700/uboot.fragment`. Patch 0006 gives `reg_dcdc3` a range of 1.1V to
  1.2V, and Linux leaves a voltage inside its range alone, so it keeps the SPL's. U-Boot
  proper needs the same range (U-Boot patch 0002): its regulator uclass sets an
  always-on regulator whose minimum equals its maximum, and set 1.1V again after the SPL.
  Tested on the LPDDR3 test unit (2026-11-18): the controller came up as LPDDR3 on the
  full bus with one rank (MSTR `0xC1040008`), 1 GiB, `vdd-dram` at 1.2V through 3 reboots
  and a lid sleep. The time before the kernel was 2.74 to 3.03 s over four boots, against
  2.53 to 2.66 s recorded before, so the failed LPDDR4 try costs about 0.1 to 0.2 s. Not
  tested: any LPDDR4 unit, a cold start from power-off with this SPL.
- **The RAM type is reported** for testers (`device/ram.rs`): the launcher reads the DRAM
  controller's MSTR register (`0x047FB000` on the H616, H618 and H700, found by the chip's
  name in the device tree's `compatible`) through `/dev/mem` once, and its device type bits
  give the type. SYSTEM INFO's MEMORY row shows it, the launcher's log starts with
  `ram: LPDDR3, vdd-dram 1.20 V`, SAVE LOGS TO CARD's `system.txt` has it, and
  `bootlog.txt` has a `== ram` section from `S99bootlog` (type, bus width, ranks, MSTR,
  the voltage and `MemTotal`), written even if the launcher doesn't start. Checked on the
  LPDDR3 unit (2026-11-18): all four show LPDDR3 at 1.20 V; the German row wasn't seen
  on the device, but `1 GB LPDDR3 - 856 MB FREI` is 25 of the 28 characters it has.
- **A RAM setup failure resets the chip.** The first boot stage turns the LED on,
  then panics and resets, which shows as a 1 Hz green blink with nothing on the card.
- **EHCI0 and OHCI0 share PHY 0 with the MUSB device controller.** If they run, they
  switch the port to host mode and the USB console never connects, so patch 0007
  disables them. USB host mode needs that reverted, plus a dual-role setup like
  ROCKNIX's `0151` and `0152` patches. Those also manage a 5V output on the port, so
  review them before use while the port is connected to a PC.
- **The USB controller has 4 IN and 4 OUT endpoints,** besides the control endpoint.
  Serial and Ethernet take 2 IN and 1 OUT each, so together 4 IN and 2 OUT, and any
  further USB function needs one of them dropped: USB STORAGE drops the serial port.
- **Mainline's MUSB gadget driver throws away USB packets that arrive early.** Since
  commit 3fc137386c46, `musb_ep_restart` flushes an RX packet that came before the gadget
  queued its request. Mass storage lost commands that way, and the PC reset the device
  every 30 seconds. Patch 0014 reads the packet instead.
- **Save states keep mGBA's Super Game Boy border mode.** A state saved with the border
  on loads into the borderless frame offset and cut off, so such states can't be used
  since the border was turned off. Red's were deleted on 2026-11-08.
- **The CPU governor raises the speed slowly from idle while a game runs,** because a
  game waits for the panel between frames. After a pause, games ran at 30 fps with audio
  underruns for about a second, so the menu holds `scaling_min_freq` at 1008 MHz until
  two seconds after resuming.
- **Plugging in USB power starts the device** (VBUS rising, set in the AXP717's efuse;
  powering off with the cable in stays off). REG 0x20 says what started it; the
  charging screen uses it. Test shutdown with the cable unplugged.
- **Kernel power-off:** `axp20x_power_off` writes register `0x32`, but the AXP717
  turns off through `0x27`. Shutdown works because it falls through to PSCI, and
  TF-A then writes the correct register.
- **X and Y pins:** the face buttons are X top, Y left, A right, B bottom, as confirmed
  by the user. The shared device tree has X on PA3 and Y on PA2, but this unit has them
  the other way round, so patch 0008 swaps the pins. Another SP may differ.
- **Kernel modules are off, and nothing could load them anyway,** since there is no
  `mdev` or `eudev`. Add new drivers to `linux.fragment` as `=y`. Don't turn
  `CONFIG_MODULES` off in a fragment over a config that has `=m` entries: each one
  becomes built in, which added 1,377 drivers in a trial.
- **Audio levels:** `amixer` percentages are raw steps, not loudness. `DAC` at 50% and
  `Line Out` at 50% total about -58 dB, which sounded silent. If the `DAC` switch is off,
  the DAC never runs and playback fails with error -5.
- **Headphones are silent while the `Speaker` path is on,** so the two can't play at once.
- **Opening the sound card after about 5 s idle takes 700 ms longer:** `msleep(700)` in
  `sun4i_codec_spk_event` in the kernel's `sun4i-codec.c`, waiting for the amp. Measured
  with 1.0 s of silence through aplay: 1.80 s after 12 s idle, 1.08 s straight after
  another sound or after 2 s idle.
- **The amp stays on for 5 s after a sound** (`pmdown_time`; PI5 measured high for 5.29 s),
  while BusyBox `init` pauses only 2 s between the shutdown scripts and power-off. A
  shutdown right after a menu sound or a game could therefore cut the power with the amp
  on, which made the speaker crackle on some shutdowns. `S20audio stop` turns off
  `Line Out`, then `Speaker`, after the launcher has exited, which drops PI5 at once
  (0.02 s measured), and the user no longer hears the crackle. The controls are back on
  after a reboot (checked).
- **The display is `card1`,** because Panfrost now takes `card0`. Name it explicitly, as
  in `kmscube -D /dev/dri/card1`, and stop the launcher first with
  `/etc/init.d/S90launcher stop`.
- **`kmscube` quits on any waiting input,** printing `user interrupted!`. Paste its test
  as one line, because lines pasted after it count as input. Over SSH, closed input also
  counts, so give it open input: `sleep 30 | kmscube -D /dev/dri/card1 -c 600`.
- **The GPU needs `CONFIG_SUN50I_H6_PRCM_PPU`.** It has no default, and without it
  Panfrost fails silently, with only `ignoring dependency` in the log.
- **TETSU-DATA starts at a fixed 4 GiB,** past the image, so writing the image can't reach it.
  Writing the image does replace the partition table, so `flash.sh` adds the entry back,
  as partition 4 since 2026-11-12 (3 before); the system finds it by its label.
  After an image writer or a plain `dd`, the device adds it at its next start
  (`/usr/sbin/data-partition`, see the card layout).
- **The SP has one of two panels, found at each start** (kernel patch 0020). ROCKNIX ships a
  device tree for each and leaves the choice to the user. Here the NV3052C driver resets the
  panel when it probes and reads its display ID (`0x04`) over the 3-wire SPI bus. An NV3052C
  reports `30 52 01` after a reset and `40 00 03` once set up. Any other ID, still after a
  second read 130 ms later, gets the second panel's setup and timings, from ROCKNIX's
  `anbernic,rg35xx-sp-v2-panel.panel`; an ID that can't be read keeps the NV3052C's. Only
  boards with the new `anbernic,rg35xx-sp-panel` compatible do this. The log says what it
  found, `panel_newvision_nv3052c.panel=nv3052c` or `=v2` on the command line overrides it, and
  the launcher reads the same parameter in sysfs: games are paced at the second panel's
  59.52 Hz, and 50 HZ GAMES AT 50 HZ does nothing on it, as nobody has seen that panel take
  the longer blanking. Tested on the NV3052C unit (2026-11-18): found on 8 of 8 starts, the
  launcher starting 1.45 to 1.75 s after the kernel as before. With `=v2` forced there, the
  kernel set the second panel's mode (640 664 672 720 / 480 512 516 560) and blanking and
  unblanking gave no errors. The user saw the picture normal and back after a lid close.
  Not tested: any real second panel, its picture or its ID.
- **Wi-Fi blocks sleep without `keep-power-in-suspend`.** rtw88 fails its SDIO
  suspend with `-EINVAL` ("Failed to host PM flag MMC_PM_KEEP_POWER"), which aborts
  the whole suspend, so lid sleep stops working. Patch 0013 adds the property to `mmc1`.
- **Sleeping while connected to Wi-Fi hangs the device for good.** The kernel logs
  `PM: suspend entry` and never finishes suspending: programs stay frozen, so SSH and
  the console don't answer, but ping still does, and only holding the power button
  helps. Not connected, it sleeps fine. `sleep_until_lid_opens` therefore sends
  `DISCONNECT` before sleeping and `RECONNECT` after. The hanging driver call wasn't
  found: programs are frozen by then, and kernel messages go nowhere (`ttynull`), so
  finding it would need netconsole over `usb0`.
- **This `wpa_supplicant` has no syslog support:** `-s` prints the usage and exits
  with status 0, so a script can't tell it failed.
- **The linux-sunxi wiki returns 403 to automated fetches.** Use U-Boot's
  `doc/board/allwinner/sunxi.rst`, the ROCKNIX tree and the source code instead.
