// The devices a release has a card image for, as scripts/device-images.sh names them:
// tetsu-<name>[-rev6]-<version>-sdcard.img.gz. A new device in
// BR2_LINUX_KERNEL_INTREE_DTS_NAME needs a line here too (docs/guide/requirements.md has
// the same table for readers).
export interface Device {
  /** The name in the image's file name, from the device tree's */
  name: string
  /** What is printed on the device */
  label: string
  /** Has a -rev6 image, for newer units (board revision 6) with another screen */
  rev6?: boolean
  /** Started and checked on a real unit */
  tested?: boolean
  note?: string
}

export const devices: Device[] = [
  { name: 'rg35xx-sp', label: 'RG35XX SP', tested: true, note: 'LPDDR3 and LPDDR4 units' },
  { name: 'rg35xx-plus', label: 'RG35XX Plus', rev6: true },
  { name: 'rg35xx-h', label: 'RG35XX H', rev6: true },
  { name: 'rg35xx-2024', label: 'RG35XX 2024', rev6: true, note: 'No Wi-Fi: no updates, Bluetooth or Wi-Fi transfer' },
  { name: 'rg35xx-pro', label: 'RG35XX Pro' },
  { name: 'rg40xx-h', label: 'RG40XX H' },
  { name: 'rg40xx-v', label: 'RG40XX V' },
  { name: 'rg34xx', label: 'RG34XX' },
  { name: 'rg34xx-sp', label: 'RG34XX-SP' },
  { name: 'rgcubexx', label: 'RG CubeXX', note: 'The menus use the top of the square screen for now' },
]

export const REPO = 'iDentitY4/TetsuOS'
