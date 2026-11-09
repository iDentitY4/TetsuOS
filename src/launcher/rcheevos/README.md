# rcheevos

[rcheevos](https://github.com/RetroAchievements/rcheevos) v12.5.0 (MIT, see `LICENSE`),
the RetroAchievements library, from the release tarball
(sha256 `e6df83de4e18f0a19206e711e1e589624dcfcf6bf529c14f6dd2bb2d1ced983f`). `build.rs`
compiles it into the launcher. Only `include/` and `src/` are kept, without the files for
RAIntegration and external clients, which are Windows-only, and for encrypted 3DS games
(`rhash/aes.*`, `rhash/hash_encrypted.c`), which `RC_HASH_NO_ENCRYPTED` leaves unused. `libretro/libretro.h` is the
part of libretro.h that `rc_libretro.c` needs.

To update: unpack the new release over `include/` and `src/`, remove the same files, and
check `CHANGELOG.md` there for changes to the structs `achievements.rs` mirrors.
