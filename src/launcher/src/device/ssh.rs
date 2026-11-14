//! SSH access and the root password, from Settings > ADVANCED. The launcher keeps the
//! keys and the password hash on the data partition and restarts the server, whose init
//! script, `S50dropbear`, copies them into place and picks where it listens. The rules
//! here mirror that script's, so the screen shows what the server does.

use crate::settings::SshAccess;
use crate::device::storage::write_atomic;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

pub const DATA_DIR: &str = crate::device::paths::data_path!("/ssh");
const KEYS_FILE: &str = "authorized_keys";
const PASSWORD_FILE: &str = "root-password";
const SERVER: &str = "/etc/init.d/S50dropbear";
pub const MAX_PASSWORD_LENGTH: usize = 32;
/// A made-up password's characters: lowercase and digits, without ones that look alike
/// on paper (0 and o, 1, i and l) or in the 5x7 font, whose lowercase letters have no
/// descenders, so p and y read as P and Y, and q and g as 9. 27 of them.
const RANDOM_ALPHABET: &[u8] = b"abcdefhjkmnrstuvwxz23456789";
/// Groups of four, joined by dashes: 12 characters, about 57 bits.
const RANDOM_GROUPS: usize = 3;

/// What secures a login.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Login {
    pub password: bool,
    pub keys: usize,
}

impl Login {
    pub fn read() -> Self {
        Self::read_from(Path::new(DATA_DIR))
    }

    pub fn read_from(directory: &Path) -> Self {
        let read = |name: &str| std::fs::read_to_string(directory.join(name)).unwrap_or_default();
        Self { password: valid_hash(&read(PASSWORD_FILE)), keys: count_keys(&read(KEYS_FILE)) }
    }

    /// Whether the server may listen beyond the cable, where an empty password isn't allowed.
    pub fn allows_wifi(self) -> bool {
        self.password || self.keys > 0
    }
}

/// Keys in an `authorized_keys` file: lines that aren't blank or comments.
pub fn count_keys(text: &str) -> usize {
    text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')).count()
}

/// Whether the file's first line is a crypt(3) hash, as `S50dropbear` checks.
pub fn valid_hash(text: &str) -> bool {
    let first = text.lines().next().unwrap_or_default();
    first.starts_with('$') && !first.contains(':')
}

/// The access that is in effect. With nothing to secure it, Wi-Fi falls back to USB, and
/// on a release build, which allows no empty password at all, every access to off.
pub fn effective(access: SshAccess, login: Login, release: bool) -> SshAccess {
    if login.allows_wifi() || access == SshAccess::Off {
        access
    } else if release {
        SshAccess::Off
    } else if access == SshAccess::Wifi {
        SshAccess::Usb
    } else {
        access
    }
}

/// The access `steps` away, passing over the ones that can't be chosen.
pub fn next_access(current: SshAccess, steps: i32, login: Login, release: bool) -> SshAccess {
    let mut next = current.cycle(steps);
    // Off can always be chosen, so this ends.
    while effective(next, login, release) != next {
        next = next.cycle(steps.signum());
    }
    next
}

/// Whether this is a release build, from `VARIANT_ID=release` in `os-release`, which
/// `post-build.sh` writes when built with `TETSU_RELEASE=1`. Release builds start with SSH
/// off and never allow root in without a password, over SSH or the serial console.
pub fn release() -> bool {
    static RELEASE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *RELEASE.get_or_init(|| {
        std::fs::read_to_string(crate::device::paths::OS_RELEASE).is_ok_and(|text| parse_release(&text))
    })
}

pub fn parse_release(os_release: &str) -> bool {
    os_release.lines().any(|line| matches!(line.trim(), "VARIANT_ID=release" | "VARIANT_ID=\"release\""))
}

/// Whether the on-screen keyboard can type it: printable ASCII, as the font draws.
pub fn valid_password(password: &str) -> bool {
    password.chars().all(|c| matches!(c, ' '..='~'))
}

/// A made-up password like `k7mb-x3ra-9fwe`, from the kernel's random numbers.
pub fn random_password() -> io::Result<String> {
    use std::io::Read;
    let mut random = std::fs::File::open("/dev/urandom")?;
    let mut bytes = [0; 64];
    random.read_exact(&mut bytes)?;
    password_from(bytes.into_iter()).ok_or_else(|| io::Error::other("not enough random bytes"))
}

/// Makes the password from random bytes. Bytes past the largest multiple of the alphabet's
/// size are skipped, so every character is equally likely.
pub fn password_from(bytes: impl Iterator<Item = u8>) -> Option<String> {
    let size = RANDOM_ALPHABET.len();
    let limit = 256 - 256 % size;
    let mut characters =
        bytes.filter(|&byte| usize::from(byte) < limit).map(|byte| RANDOM_ALPHABET[usize::from(byte) % size]);
    let groups: Option<Vec<String>> = (0..RANDOM_GROUPS)
        .map(|_| (0..4).map(|_| characters.next().map(char::from)).collect::<Option<String>>())
        .collect();
    Some(groups?.join("-"))
}

/// Hashes the password with SHA-512 crypt and stores it. `BusyBox`'s mkpasswd reads it from
/// standard input, so it never shows in the process list.
pub fn set_password(password: &str) -> io::Result<()> {
    let mut child = Command::new("mkpasswd")
        .args(["-m", "sha512"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    child.stdin.take().ok_or_else(|| io::Error::other("no stdin"))?.write_all(password.as_bytes())?;
    let output = child.wait_with_output()?;
    let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || !hash.starts_with("$6$") || !valid_hash(&hash) {
        return Err(io::Error::other(format!("mkpasswd gave no hash: {}", output.status)));
    }
    std::fs::create_dir_all(DATA_DIR)?;
    write_atomic(&Path::new(DATA_DIR).join(PASSWORD_FILE), format!("{hash}\n").as_bytes())
}

fn remove(name: &str) -> io::Result<()> {
    match std::fs::remove_file(Path::new(DATA_DIR).join(name)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

pub fn remove_password() -> io::Result<()> {
    remove(PASSWORD_FILE)
}

pub fn remove_keys() -> io::Result<()> {
    remove(KEYS_FILE)
}

/// Starts the server again with the current settings, keys and password. Open sessions
/// stay connected.
pub fn restart_server() {
    match Command::new(SERVER).arg("restart").output() {
        Ok(output) => print!("{}", String::from_utf8_lossy(&output.stdout)),
        Err(error) => eprintln!("warning: could not restart the SSH server: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_hashes_are_read_like_the_init_script() {
        assert_eq!(count_keys("ssh-rsa AAAA user@pc\n\n# old\nssh-ed25519 BBBB\n"), 2);
        assert_eq!(count_keys(""), 0);
        assert!(valid_hash("$6$salt$hash\n"));
        assert!(!valid_hash("plain text\n"));
        assert!(!valid_hash("$6$a:b\n"));
        assert!(!valid_hash(""));
    }

    #[test]
    fn wifi_needs_a_key_or_a_password() {
        let none = Login::default();
        let key = Login { keys: 1, ..Login::default() };
        let password = Login { password: true, ..Login::default() };
        assert!(!none.allows_wifi() && key.allows_wifi() && password.allows_wifi());
        assert_eq!(effective(SshAccess::Wifi, none, false), SshAccess::Usb);
        assert_eq!(effective(SshAccess::Wifi, key, false), SshAccess::Wifi);
        assert_eq!(effective(SshAccess::Off, none, false), SshAccess::Off);
        // Off, USB, Wi-Fi, wrapping: without a key, Wi-Fi is passed over both ways.
        assert_eq!(next_access(SshAccess::Usb, 1, none, false), SshAccess::Off);
        assert_eq!(next_access(SshAccess::Off, -1, none, false), SshAccess::Usb);
        assert_eq!(next_access(SshAccess::Usb, 1, password, false), SshAccess::Wifi);
        assert_eq!(next_access(SshAccess::Off, -1, key, false), SshAccess::Wifi);
    }

    #[test]
    fn a_release_build_allows_no_login_without_a_password_or_key() {
        let none = Login::default();
        let password = Login { password: true, ..Login::default() };
        for access in SshAccess::ALL.iter().copied() {
            assert_eq!(effective(access, none, true), SshAccess::Off);
            assert_eq!(effective(access, password, true), access);
        }
        // Nothing to step to without a login: it stays off both ways.
        assert_eq!(next_access(SshAccess::Off, 1, none, true), SshAccess::Off);
        assert_eq!(next_access(SshAccess::Off, -1, none, true), SshAccess::Off);
        assert_eq!(next_access(SshAccess::Off, 1, password, true), SshAccess::Usb);
    }

    #[test]
    fn a_release_build_is_named_in_os_release() {
        assert!(parse_release("NAME=\"Tetsu OS\"\nVARIANT_ID=release\nBUILD_ID=1\n"));
        assert!(parse_release("VARIANT_ID=\"release\"\n"));
        assert!(!parse_release("NAME=\"Tetsu OS\"\nBUILD_ID=1\n"));
    }

    #[test]
    fn made_up_passwords_are_groups_of_four_from_the_alphabet() {
        let password = password_from((0..=255).cycle().take(64)).unwrap();
        assert_eq!(password, "abcd-efhj-kmnr");
        // 243 and above would favour the first letters, so they are skipped.
        let skipped = password_from([243, 255, 30].into_iter().chain(std::iter::repeat(0)).take(20)).unwrap();
        assert!(skipped.starts_with('d'), "{skipped}");
        assert_eq!(password_from([255; 64].into_iter()), None);
        let password = random_password().unwrap();
        assert_eq!(password.len(), 14);
        assert!(valid_password(&password));
        assert!(password.chars().all(|c| c == '-' || RANDOM_ALPHABET.contains(&(c as u8))));
        assert_ne!(random_password().unwrap(), password);
    }

    #[test]
    fn passwords_are_printable_ascii() {
        assert!(valid_password("Correct horse 9!"));
        assert!(!valid_password("caf\u{e9}"));
        assert!(!valid_password("tab\there"));
    }

    #[test]
    fn the_login_is_read_from_its_folder() {
        let directory = std::env::temp_dir().join(format!("launcher-ssh-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        assert_eq!(Login::read_from(&directory), Login::default());
        std::fs::write(directory.join(KEYS_FILE), "ssh-rsa AAAA user@pc\n").unwrap();
        std::fs::write(directory.join(PASSWORD_FILE), "$6$salt$hash\n").unwrap();
        assert_eq!(Login::read_from(&directory), Login { password: true, keys: 1 });
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
