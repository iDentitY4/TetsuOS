# Troubleshooting

## The device doesn't start

**Nothing on the screen, green light blinking about once a second.** The memory setup
failed. With v0.1, the unit most likely has LPDDR4 memory, which v0.1 can't set up: see
[Check your memory type](./requirements#check-your-memory-type). Later builds try both
types, so with one of them, please [report it](./testing#_4-send-the-report).

**Nothing at all.** Check that the card is in **TF1**, and write it again. Make sure the
battery has some charge: plug in the cable for a few minutes.

**The logo shows, but the launcher never comes.** Wait a minute, then take the card out and
look at `bootlog.txt` on the **TETSU-BOOT** partition with a computer. It is written about 15
and 60 seconds after Linux starts. No `bootlog.txt` means Linux didn't get that far.

To see the kernel's messages while starting, set **Settings > ADVANCED > BOOT MESSAGES** to
TEXT on a working boot.

**It stays on the logo, and the power button does nothing.** The card wasn't recognised
properly. Since v0.1 the device tries again by itself within a few seconds. If it still
hangs, take the card out and put it back while the logo shows: the start carries on. Please
report it with a photo of the screen, taken with BOOT MESSAGES set to TEXT.

**After an update, the old version is back.** The new system didn't reach the launcher in
three tries, and the device went back to the old one by itself. SYSTEM UPDATE says
`<build> DID NOT START, THIS SYSTEM WAS KEPT`. Save logs (Settings > SYSTEM > SYSTEM INFO, X)
and report it.

## CARD ERROR

The card's TETSU-DATA partition became read-only, usually after damage from a power cut,
so nothing could be saved. Press **A** to restart, which repairs it. TETSU-DATA is also
checked and repaired at every start after an unclean shutdown.

If it keeps happening, copy your saves off the card and try another one.

## The setup shows at every start

TETSU-DATA couldn't be mounted, so the settings are missing. Check that the card has the
TETSU-DATA partition (see [Installation](./installation#_2-write-the-card)), and run a check
on it from a computer.

## Games

**A game doesn't appear.** Check its extension against
[Supported systems](../reference/systems). Mega Drive `.bin` files need renaming to `.md`.
Hidden files and folders (starting with a dot) are skipped. A damaged or password-protected
ZIP is left out.

**A PlayStation disc appears twice, or a `.bin` appears.** Keep each `.bin` with its `.cue`,
and check that the disc names differ only in the disc number for grouping.

**Sound crackles or stutters.** An occasional single gap in sound happens, especially at a
game's start. If a game stutters all the time, try **PERFORMANCE** in Settings > ADVANCED.

**A 50 Hz game looks slightly uneven.** European (PAL) games, the Vectrex and the Virtual
Boy run at 50 Hz on a 60 Hz screen, so some frames show twice.

**An old save state loads cut off.** Game Boy states saved before Super Game Boy borders were
turned off can't be used.

## Wi-Fi

**A network shows NOT SUPPORTED.** WPA3-only, enterprise and WEP networks don't work.
Switch the router to WPA2 or WPA2/WPA3 mixed mode.

**Connected, but nothing loads.** Restart the device. Twice, the Wi-Fi chip stopped sending
while staying connected; turning Wi-Fi off and on didn't help, a restart did.

**Box art, cheats or achievements say there is no connection.** Check that the clock is
right: secure connections fail with a wrong date. Set the time zone, or SET TIME by hand.

## USB

**The computer doesn't see the device.** Use a data cable, not a charge-only one. The device
shows up as a network adapter (and a serial port) as soon as it starts.

**USB STORAGE doesn't leave.** Eject the drive (and the TF2 drive) on the computer first.

**A faint high-pitched noise with USB connected** was heard once and not since.

## Getting logs

- **Settings > SYSTEM > SYSTEM INFO**, then **X**, saves `launcher.log`, `dmesg.txt` and
  `system.txt` to `logs` on TETSU-BOOT, without passwords or keys.
- `bootlog.txt` on TETSU-BOOT has each start's log, with the RAM's type and voltage.
- Over USB, see [Device access](../developer/device-access) for SSH and the serial console.

Please include them when [reporting an issue](https://github.com/iDentitY4/TetsuOS/issues).
