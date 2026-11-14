# Contributing to Tetsu OS

Thanks for helping. Bug reports, testing on other RG35XX SP units, translations and pull
requests are all welcome. The [developer guide](https://identity4.github.io/TetsuOS/developer/)
explains the build, the launcher's code and the hardware; its
[Contributing](https://identity4.github.io/TetsuOS/developer/contributing) page has the
details for translations, new consoles and new settings.

## Reporting a bug

Open an [issue](https://github.com/iDentitY4/TetsuOS/issues/new/choose) with the bug form.
The most useful reports say which build you run (Settings > SYSTEM > SYSTEM INFO), your
unit's memory type, what you did and what happened, and include the logs: X on Settings >
SYSTEM > SYSTEM INFO saves them to the card's TETSU-BOOT drive.

Please don't link to or attach games or BIOS files.

Security problems go through [private reporting](SECURITY.md), not public issues.

## Pull requests

1. For anything larger than a fix, open an issue first, so we can agree on the approach.
2. Build and test:

   ```sh
   cd src/launcher
   cargo test
   cargo clippy --all-targets   # stays clean
   ```

   Kernel and package patches must apply with Buildroot's
   `support/scripts/apply-patches.sh`. A full image is `make` at the top (see
   [Building](https://identity4.github.io/TetsuOS/developer/building)).
3. Say what you tested on a real device and what you didn't. Sound, the lid, sleep, Wi-Fi
   and anything you hold in your hands can only be checked there.
4. Update the docs: `docs/features.md` for how a feature works and was tested, and the
   user guide pages it changes. Text on screen needs its German line in
   `src/launcher/lang/de.txt` (`cargo test i18n` checks).
5. Keep a pull request to one topic, with commits that say why in plain words.

`CLAUDE.md` sums up the conventions and what reviews look at.

## Licence

By contributing, you agree that your contribution is licensed under the
[MIT license](LICENSE), like the rest of Tetsu OS's own code. Patches to the kernel, BlueZ
and other projects keep those projects' licences.

## Conduct

Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md).
