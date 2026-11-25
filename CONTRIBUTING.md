# Contributing to Tetsu OS

Thanks for helping. Bug reports, testing on other RG35XX SP units, translations and pull
requests are all welcome. The [developer guide](https://tetsu-os.org/developer/)
explains the build, the launcher's code and the hardware; its
[Contributing](https://tetsu-os.org/developer/contributing) page has the
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
   [Building](https://tetsu-os.org/developer/building)).
3. Say what you tested on a real device and what you didn't. Sound, the lid, sleep, Wi-Fi
   and anything you hold in your hands can only be checked there.
4. Update the docs: `docs/features.md` for how a feature works and was tested, and the
   user guide pages it changes. Text on screen needs its German line in
   `src/launcher/lang/de.txt` (`cargo test i18n` checks).
5. Keep a pull request to one topic, with commits that say why in plain words.

`CLAUDE.md` sums up the conventions and what reviews look at.

## How a change gets into main

`main` only takes pull requests: nobody pushes to it directly.

1. **Branch** from an up-to-date `main`, with a short name for the topic:

   ```sh
   git switch main && git pull
   git switch -c fix-wifi-stall
   ```

   Contributors without write access work on a fork the same way.
2. **Commit and push** the branch, and open a pull request. The template asks what was
   tested.
3. **Checks:** the `launcher` check (`.github/workflows/ci.yml`: the launcher's tests
   and clippy) must pass, on a branch that is up to date with `main`; GitHub's **Update
   branch** button brings it up to date. A Claude review comments on the changes too.
4. **Review:** the maintainer approves as code owner, after the last push, with every
   review thread resolved.
5. **Merge:** squash only, so each pull request becomes one commit on `main`, titled after
   the pull request. With **Enable auto-merge**, it merges by itself once the checks
   pass. GitHub then deletes the branch.
6. **Clean up** your copy:

   ```sh
   git switch main && git pull && git fetch --prune
   git branch -D fix-wifi-stall   # -D: git can't tell a squashed branch was merged
   ```

Releases are tags on `main` (`v0.1`, `v0.2`, ...); see
[Making a release](https://tetsu-os.org/developer/releases).

## Licence

By contributing, you agree that your contribution is licensed under the
[MIT license](LICENSE), like the rest of Tetsu OS's own code. Patches to the kernel, BlueZ
and other projects keep those projects' licences.

## Conduct

Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md).
