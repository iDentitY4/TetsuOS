## What and why

<!-- What this changes, and why. Link the issue: "Fixes #123". -->

## Tested

<!-- What you ran, and what you checked on a real device (which unit and memory type).
     Say what wasn't tested: sound, the lid, sleep and Wi-Fi only show on the device. -->

- [ ] `cargo test` and `cargo clippy --all-targets` pass in `src/launcher/`
- [ ] On a device:

## Checklist

- [ ] Text on screen goes through `tr`/`trf`, with German lines in `lang/de.txt`
- [ ] `docs/features.md` and the user guide pages it changes are updated
- [ ] Kernel or package patches apply with Buildroot's `apply-patches.sh`
- [ ] Old `settings.txt` files and saves still load
- [ ] `CHANGELOG.md` has a line under Unreleased, if users will notice
