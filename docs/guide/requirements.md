# Requirements

## The device

An **Anbernic RG35XX SP** (Allwinner H700).

Builds after v0.1 also carry the other H700 devices below. Nobody has started one of
them yet: they are experimental. Each release has a card image per device, named after
it, such as `tetsu-rg40xx-h-<version>-sdcard.img.gz`: download the one for yours
([installation](./installation#_1-download-an-image)).

| Device | Name | Notes |
| --- | --- | --- |
| RG35XX SP | `rg35xx-sp` | Tested |
| RG35XX Plus | `rg35xx-plus` | |
| RG35XX H | `rg35xx-h` | The left stick works like the D-pad |
| RG35XX 2024 | `rg35xx-2024` | No Wi-Fi: the menus leave out Wi-Fi, Bluetooth and what needs them, SYSTEM UPDATE too |
| RG35XX Pro | `rg35xx-pro` | Left stick as the D-pad |
| RG40XX H | `rg40xx-h` | Left stick as the D-pad; no RGB lights or vibration yet |
| RG40XX V | `rg40xx-v` | As the RG40XX H |
| RG34XX | `rg34xx` | 720x480 screen |
| RG34XX-SP | `rg34xx-sp` | 720x480 screen, left stick as the D-pad |
| RG CubeXX | `rgcubexx` | 720x720 screen: the menus use its top 480 rows for now; no RGB lights yet |

Games don't get analog input yet. Newer units of the Plus, H and 2024 (board revision 6)
have another screen panel, which can't be told from the older one: if the screen stays
dark, write the device's `-rev6` image instead, such as
`tetsu-rg35xx-plus-rev6-<version>-sdcard.img.gz`. The RG28XX, whose screen is mounted
turned, isn't in the image yet.

### Check your memory type

RG35XX SP units ship with either LPDDR3 or LPDDR4 memory, and the bootloader has to set
up the right one.

- **v0.1 is built for LPDDR3.** On an LPDDR4 unit it doesn't start, and the green light
  blinks about once a second.
- **Builds after v0.1** try LPDDR4, then LPDDR3, so one image starts on both. One
  tester's LPDDR4 SP has run v0.2-test.2: a game with sound, headphones, sleep and resume,
  Wi-Fi and the power button worked ([issue #18](https://github.com/iDentitY4/TetsuOS/issues/18)). More reports are welcome:
  [help test](./testing). Settings > SYSTEM > SYSTEM INFO shows the memory type on the
  MEMORY row.

You can read the memory type from the stock firmware card (or a muOS card) that boots on
your unit. The first boot stage, boot0, starts 8 KiB into the card, and the 32-bit value at
offset `0x3c` in it is the type: **7 is LPDDR3, 8 is LPDDR4**. On Linux, with the card in
a reader at `/dev/sdX`:

```bash
sudo dd if=/dev/sdX bs=1 skip=$((8192 + 0x3c)) count=4 2>/dev/null | od -An -tu4
```

If it prints `8`, your unit has LPDDR4: v0.1 won't start on it, but the
[test image](./testing) should.

::: tip
Trying a card costs nothing: a card that doesn't boot does no harm, and the stock card
still works afterwards. The internal storage is not touched.
:::

## A microSD card

- **At least 6 GB.** 16 GB or more is better, as the games go on the same card.
- The card goes in the **TF1** slot (the one the stock firmware boots from).
- Installing **erases the card**. Use a spare one, not the stock firmware card.

A second card in **TF2** is optional and adds more games. It can be FAT32 or exFAT, and
doesn't need anything from Tetsu OS. See [A second card](./adding-games#a-second-card-in-tf2).

## A computer

Any computer with a card reader can write the card. To build Tetsu OS yourself you need
Linux; see [Building from source](../developer/building).

A **USB-C cable** to a computer is useful but not required. Over the cable the device shows
up as a USB drive for copying games, and as a network adapter for SSH and updates.

## Optional

- A **Wi-Fi network** (WPA2, 2.4 or 5 GHz) for network time, box art, cheats,
  RetroAchievements, Wi-Fi transfer, save backup and updates. WPA3-only and enterprise
  networks are not supported.
- A **Bluetooth controller** or **headphones**.
- A **RetroAchievements account** from [retroachievements.org](https://retroachievements.org).
