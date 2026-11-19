//! An update's `manifest.txt`, made and signed by `scripts/make-update.sh`: which device
//! and build it is, the compressed image's file name and size, and the image's size and
//! SHA-256. `manifest.sig` is its Ed25519 signature in hex, checked against the public key
//! built in from `update-key.pub`, so only an update signed with the project's private key
//! is installed, whatever server it came from and over plain HTTP too.

use ring::signature::{UnparsedPublicKey, ED25519};

/// The public half of the signing key, as `make-update.sh --new-key` wrote it.
const PUBLIC_KEY_HEX: &str = include_str!("../../update-key.pub");

/// The manifest format this launcher reads.
const FORMAT: &str = "1";

#[derive(Clone, PartialEq, Debug)]
pub struct Manifest {
    /// The device tree's name for the first device it is for, such as `anbernic,rg35xx-sp`:
    /// the only one launchers from before `devices` (v0.1) install for.
    pub device: String,
    /// Every device it is for, separated by spaces in the manifest; just `device` in a
    /// manifest from before an image served several.
    pub devices: Vec<String>,
    /// The build's time in UTC, `20261113-193012`, as in its `os-release` `BUILD_ID`.
    pub build: String,
    /// The project's git revision, as SYSTEM INFO shows it.
    pub revision: String,
    /// The compressed image's name, next to the manifest.
    pub file: String,
    pub file_size: u64,
    /// The root filesystem image's size once unpacked, and its SHA-256 in hex.
    pub image_size: u64,
    pub image_sha256: [u8; 32],
}

pub fn from_hex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len()).step_by(2).map(|at| u8::from_str_radix(text.get(at..at + 2)?, 16).ok()).collect()
}

/// Whether `signature` (hex) is the key's signature of `manifest`.
pub fn verify_with(public_key: &[u8], manifest: &[u8], signature: &str) -> bool {
    let Some(signature) = from_hex(signature) else { return false };
    UnparsedPublicKey::new(&ED25519, public_key).verify(manifest, &signature).is_ok()
}

/// Whether `signature` is the project's signature of `manifest`.
pub fn verify(manifest: &[u8], signature: &str) -> bool {
    from_hex(PUBLIC_KEY_HEX).is_some_and(|key| verify_with(&key, manifest, signature))
}

/// The manifest's fields, if they are all there and well formed. Unknown lines are
/// skipped, so later formats can add some.
pub fn parse(text: &str) -> Result<Manifest, String> {
    let field = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
            .map(str::trim)
            .ok_or_else(|| format!("the manifest has no {name}"))
    };
    let number = |name: &str| field(name)?.parse::<u64>().map_err(|_| format!("the manifest's {name} isn't a number"));
    if field("format")? != FORMAT {
        return Err(format!("the manifest's format is {}, not {FORMAT}", field("format")?));
    }
    let build = field("build")?;
    if !is_build(build) {
        return Err(format!("the manifest's build {build:?} isn't a build time"));
    }
    let file = field("file")?;
    // A plain name next to the manifest, never a path elsewhere.
    if file.is_empty() || file.contains('/') || file.contains("..") || file.contains('?') || file.contains('#') {
        return Err(format!("the manifest's file {file:?} isn't a plain name"));
    }
    let image_sha256 = from_hex(field("image_sha256")?)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or("the manifest's image_sha256 isn't a SHA-256")?;
    let device = field("device")?.to_string();
    let devices = match field("devices") {
        Ok(list) => list.split_whitespace().map(str::to_string).collect(),
        Err(_) => vec![device.clone()],
    };
    Ok(Manifest {
        device,
        devices,
        build: build.to_string(),
        revision: field("revision").unwrap_or_default().to_string(),
        file: file.to_string(),
        file_size: number("file_size")?,
        image_size: number("image_size")?,
        image_sha256,
    })
}

impl Manifest {
    /// Whether the update is for the device with this `compatible` name.
    pub fn is_for(&self, compatible: &str) -> bool {
        self.devices.iter().any(|device| device == compatible)
    }
}

/// Whether `text` is a build time as `post-build.sh` writes it: `YYYYMMDD-HHMMSS`.
pub fn is_build(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 15
        && bytes[8] == b'-'
        && bytes.iter().enumerate().all(|(at, byte)| at == 8 || byte.is_ascii_digit())
}

/// A build time as the screen shows it, `2026-11-13 19:30 UTC`.
pub fn build_label(build: &str) -> String {
    if !is_build(build) {
        return build.to_string();
    }
    format!("{}-{}-{} {}:{} UTC", &build[..4], &build[4..6], &build[6..8], &build[9..11], &build[11..13])
}

/// Whether the update is newer than the running system. A system without a build time,
/// from before updates, is older than any.
pub fn is_newer(update: &str, running: Option<&str>) -> bool {
    match running.filter(|running| is_build(running)) {
        // The format sorts as text.
        Some(running) => update > running,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    const TEXT: &str = "format=1\ndevice=anbernic,rg35xx-sp\nbuild=20261113-193012\nrevision=5608e24\n\
        file=tetsu-h700-20261113-193012.img.gz\nfile_size=62384069\nimage_size=1073741824\n\
        image_sha256=00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff\n";

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn a_manifest_reads() {
        let manifest = parse(TEXT).unwrap();
        assert_eq!(manifest.device, "anbernic,rg35xx-sp");
        assert_eq!(manifest.build, "20261113-193012");
        assert_eq!(manifest.revision, "5608e24");
        assert_eq!(manifest.file, "tetsu-h700-20261113-193012.img.gz");
        assert_eq!((manifest.file_size, manifest.image_size), (62384069, 1073741824));
        assert_eq!(manifest.image_sha256[..3], [0x00, 0x11, 0x22]);
        // Unknown lines are skipped.
        assert!(parse(&format!("{TEXT}later=1\n")).is_ok());
    }

    #[test]
    fn an_update_is_for_its_listed_devices_or_its_one_device() {
        let old = parse(TEXT).unwrap();
        assert_eq!(old.devices, ["anbernic,rg35xx-sp"]);
        assert!(old.is_for("anbernic,rg35xx-sp") && !old.is_for("anbernic,rg35xx-plus"));
        let family = parse(&TEXT.replace(
            "device=anbernic,rg35xx-sp\n",
            "device=anbernic,rg35xx-sp\ndevices=anbernic,rg35xx-sp anbernic,rg35xx-plus anbernic,rg35xx-h\n",
        ))
        .unwrap();
        assert_eq!(family.device, "anbernic,rg35xx-sp", "devices= isn't read as device=");
        assert!(family.is_for("anbernic,rg35xx-plus") && family.is_for("anbernic,rg35xx-h"));
        assert!(!family.is_for("anbernic,rg35xx-2024") && !family.is_for("anbernic,rg35xx"));
    }

    #[test]
    fn a_bad_manifest_is_refused() {
        assert!(parse(&TEXT.replace("format=1", "format=2")).is_err());
        assert!(parse(&TEXT.replace("build=20261113-193012", "build=yesterday")).is_err());
        assert!(parse(&TEXT.replace("file=tetsu", "file=../tetsu")).is_err());
        assert!(parse(&TEXT.replace("file=tetsu", "file=http://elsewhere/tetsu")).is_err());
        assert!(parse(&TEXT.replace("file_size=62384069", "file_size=big")).is_err());
        assert!(parse(&TEXT.replace("image_sha256=00", "image_sha256=")).is_err());
        assert!(parse(&TEXT.replace("device=anbernic,rg35xx-sp\n", "")).is_err());
    }

    #[test]
    fn only_the_keys_signature_is_accepted() {
        let pair = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let key = pair.public_key().as_ref().to_vec();
        let signature = hex(pair.sign(TEXT.as_bytes()).as_ref());
        assert!(verify_with(&key, TEXT.as_bytes(), &signature));
        assert!(verify_with(&key, TEXT.as_bytes(), &format!("{signature}\n")), "the file's line end");
        assert!(!verify_with(&key, TEXT.replace("62384069", "62384070").as_bytes(), &signature), "a changed manifest");
        let other = Ed25519KeyPair::from_seed_unchecked(&[8; 32]).unwrap();
        assert!(!verify_with(other.public_key().as_ref(), TEXT.as_bytes(), &signature), "another key");
        assert!(!verify_with(&key, TEXT.as_bytes(), "zz"));
        assert!(!verify_with(&key, TEXT.as_bytes(), ""));
        // A signature not made with the project's key.
        assert!(!verify(TEXT.as_bytes(), &signature));
    }

    #[test]
    fn the_built_in_key_is_a_key() {
        assert_eq!(from_hex(PUBLIC_KEY_HEX).map(|key| key.len()), Some(32));
    }

    #[test]
    fn builds_compare_by_time() {
        assert!(is_newer("20261113-193012", Some("20261112-080000")));
        assert!(!is_newer("20261113-193012", Some("20261113-193012")), "the same build");
        assert!(!is_newer("20261112-080000", Some("20261113-193012")), "an older build");
        assert!(is_newer("20261113-193012", None), "a system from before updates");
        assert!(is_newer("20261113-193012", Some("garbage")));
        assert_eq!(build_label("20261113-193012"), "2026-11-13 19:30 UTC");
        assert_eq!(build_label("other"), "other");
    }
}
