//! Compiles rcheevos, the `RetroAchievements` library, and the cores' log function into
//! the launcher. Buildroot sets `CC` and `CFLAGS` for the device, which the `cc` crate uses.

fn main() {
    let root = std::path::Path::new("rcheevos");
    let mut files = Vec::new();
    for dir in ["src", "src/rapi", "src/rcheevos", "src/rhash"] {
        for entry in std::fs::read_dir(root.join(dir)).expect("the rcheevos sources are in src/launcher/rcheevos") {
            let path = entry.expect("the rcheevos sources can be listed").path();
            if path.extension().is_some_and(|extension| extension == "c") {
                files.push(path);
            }
        }
    }
    files.sort();
    // Only the layout test uses this one.
    files.push("rcheevos_layout.c".into());
    cc::Build::new()
        .files(&files)
        .include(root.join("include"))
        .include(root.join("libretro"))
        // Identifying games from their files, without the 3DS's encrypted ones.
        .define("RC_CLIENT_SUPPORTS_HASH", None)
        .define("RC_HASH_NO_ENCRYPTED", None)
        .warnings(false)
        .compile("rcheevos");
    cc::Build::new().file("core_log.c").compile("core_log");
    println!("cargo:rerun-if-changed=core_log.c");
    println!("cargo:rerun-if-changed=rcheevos");
    println!("cargo:rerun-if-changed=rcheevos_layout.c");
}
