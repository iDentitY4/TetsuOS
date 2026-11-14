# Security policy

## Reporting a vulnerability

Please report security problems **privately**, not in a public issue:

1. Open the repository's [Security tab](https://github.com/iDentitY4/TetsuOS/security).
2. Choose **Report a vulnerability** and describe the problem, how to reproduce it and
   which build it affects (Settings > SYSTEM > SYSTEM INFO).

Only the maintainer sees the report. You'll get an answer within a week. Once a fix is
released, the advisory is published, crediting you unless you'd rather not be named.

## Supported versions

Only the newest release gets fixes, delivered through SYSTEM UPDATE. Development builds
made from the source aren't supported.

## What counts

For example:

- A way to get past the update signature check, or to make a device install an update
  that isn't from this project
- A way into the device over Wi-Fi or Bluetooth: SSH, WI-FI TRANSFER (its code and upload
  handling), Syncthing's SAVE BACKUP, mDNS
- Root login without a password on a release build (development builds allow it over the
  USB cable on purpose)
- Leaking Wi-Fi passwords, RetroAchievements tokens or other secrets from the device
- Anything in this repository's workflows that could leak a secret or let someone else
  publish a release

Not in scope: someone with the device in hand (the card can be read on any computer), and
problems in the emulator cores themselves, which belong upstream unless Tetsu OS makes
them reachable over the network.
