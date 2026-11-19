# Testing your device

Tetsu OS is developed on one RG35XX SP, which has LPDDR3 memory. Builds after v0.1 also
start on units with **LPDDR4** memory: one tester's LPDDR4 RG35XX SP has run v0.2-test.2
([issue #18](https://github.com/iDentitY4/TetsuOS/issues/18)). The test builds also carry
**other H700 devices**: the RG35XX Plus, H, 2024 and Pro, the RG40XX H and V, the RG34XX
and RG34XX-SP and the RG CubeXX, which nobody has started yet. If you have one of them, or
an SP, a test tells us whether it works. It takes about fifteen minutes, and a card that
doesn't start does no harm.

## Before you start

- **Find out your memory type** from the stock card, as shown in
  [Check your memory type](./requirements#check-your-memory-type). It helps to know what
  to expect, but you can test without it.
- **A spare microSD card** of at least 6 GB. Writing the image erases it. Don't use the
  stock firmware card.
- **The test image:** the newest pre-release marked **TEST** on the
  [Releases page](https://github.com/iDentitY4/TetsuOS/releases). Write it as shown in
  [Installation](./installation#_2-write-the-card).
- **On a device other than the RG35XX SP:** after writing the card, open its
  **TETSU-BOOT** drive on the computer and add a file `device.txt` holding your device's
  name, such as `device=rg40xx-h`. [Requirements](./requirements#the-device) lists the
  names. Without it, the device starts as an SP, which may not work on yours.

::: tip Nothing on the device changes
Tetsu OS runs from the card. The internal storage is not touched: take the card out, and
the stock firmware starts as before.
:::

## 1. Start the device

Put the card in **TF1**, unplug the USB cable and press the power button. Watch the screen
and the green light for a minute:

| What you see | What it means |
| --- | --- |
| The logo, then the setup | It works. Go on to [step 2](#_2-check-the-memory). |
| No picture, and the green light blinks about once a second | The memory setup failed: neither memory type worked. |
| The green light stays on, but the screen stays dark or garbled | The memory works, but something later doesn't, often the screen. On an RG35XX Plus, H or 2024, add a second line `panel=rev6` to `device.txt` and try again: newer units have another screen. On an SP, the build detects which of its two screens yours has, and this is how a wrong guess would look. |
| It starts, then stops about 30 seconds later | Usually a wrong name in `device.txt`: check it against your device. |
| Nothing at all | Check the card is in TF1 and the battery is charged, and try again. If that doesn't help, press the reset button once. |

If it doesn't start, take the card out and put it in your computer. A drive named
**TETSU-BOOT** appears. If it has a file named `bootlog.txt`, Linux started, and the file
says how far it got: attach it to your report. No `bootlog.txt` means Linux didn't start.

## 2. Check the memory

Go through the setup, then open **Settings > SYSTEM > SYSTEM INFO**. The **MEMORY** row
shows the memory the device found, like `1 GB LPDDR4 - 850 MB FREE`. If it says LPDDR3
on a unit whose stock card says LPDDR4, or the other way round, please say so.

## 3. Try it for a few minutes

Each of these takes a minute or two. Note anything that doesn't work as you'd expect:

- **A game:** add one as shown in [Adding games](./adding-games), play it for five minutes,
  and listen to the sound, through headphones too.
- **Buttons:** every button in a game, and in **Settings > SYSTEM > BUTTON TEST**: does
  each one say what is printed on it? Note any that are swapped, X and Y especially.
- **Sticks**, if your device has them: the left stick should move through the menus like
  the D-pad. Does it go the right way? Games don't get analog input yet.
- **The screen:** does the picture fill it, with nothing cut off or shifted? (On the RG
  CubeXX, the menus use the top of the screen for now; games fill it.)
- **Sleep:** close the lid for about a minute, open it, and carry on playing. Without a
  lid, set **Settings > SYSTEM > SLEEP WHEN IDLE** to 2 minutes, leave the menus alone
  until the screen goes dark, and wake it with the power button.
- **Wi-Fi:** connect to your network in **Settings > WI-FI** (the RG35XX 2024 has none).
- **Power:** turn it off with the power button, wait ten seconds, and turn it on again.

## 4. Send the report

On **SYSTEM INFO**, press **X**. It saves the logs to the card's **TETSU-BOOT** drive, in
`logs`. They hold no passwords or keys. Then
[open a hardware test report](https://github.com/iDentitY4/TetsuOS/issues/new?template=hardware-test.yml)
and attach `logs/system.txt`, `logs/launcher.log` and `bootlog.txt`. Say what your
`device.txt` holds, if you made one.

A report that it works is as useful as one that it doesn't. Thank you!
