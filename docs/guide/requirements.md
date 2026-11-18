# Requirements

## The device

An **Anbernic RG35XX SP** (Allwinner H700). Other H700 devices such as the RG35XX Plus,
RG40XX or RG28XX are not supported yet: they share the chip, but differ in screen,
controls and power wiring.

### Check your memory type

RG35XX SP units ship with either LPDDR3 or LPDDR4 memory, and the bootloader has to set
up the right one.

- **v0.1 is built for LPDDR3.** On an LPDDR4 unit it doesn't start, and the green light
  blinks about once a second.
- **Builds after v0.1** try LPDDR4, then LPDDR3, so one image starts on both. No LPDDR4
  unit has run one yet: if you have one, please [help test it](./testing). Once it runs,
  Settings > SYSTEM > SYSTEM INFO shows the memory type on the MEMORY row.

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
