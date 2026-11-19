//! Checking for an update and installing it, in a thread while the screen shows how far it
//! got. An install writes the slot that isn't running: the boot order first keeps that
//! slot from booting, then the image is downloaded, unpacked and written in one pass with
//! its SHA-256 taken, read back from the card and checked again, and the slot's
//! `extlinux.conf` is pointed at its own partition. Only then does the boot order put the
//! new slot first, with the running one behind it to fall back to.

use crate::device::{boot_partition, http, profile};
use crate::update::manifest::{self, Manifest};
use crate::update::slots::{self, Slot};
use flate2::read::GzDecoder;
use ring::digest::{Context, SHA256};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Where the new slot is mounted to change its `extlinux.conf`.
const SLOT_MOUNT: &str = "/run/launcher-slot";
const EXTLINUX: &str = "boot/extlinux/extlinux.conf";
/// Files a slot can't boot without.
const BOOT_FILES: [&str; 3] = ["boot/boot.scr", "boot/Image", EXTLINUX];
const CHUNK: usize = 1 << 20;
/// How often the written data is pushed to the card, so the progress is what is written.
const SYNC_EVERY: u64 = 64 << 20;
const MANIFEST_LIMIT: u64 = 64 << 10;

/// The step an install is at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    /// Downloading, unpacking and writing, counted in downloaded bytes.
    Writing,
    /// Reading the written slot back, counted in bytes read.
    Checking,
    /// Pointing the slot at its partition and changing the boot order.
    Finishing,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Status {
    Idle,
    Looking,
    /// The source's update is this system's build or older.
    UpToDate(String),
    Available(Manifest),
    Installing { step: Step, done: u64, total: u64 },
    /// Installed in the other slot, which starts at the next boot.
    Installed(Manifest),
    Failed(Failure),
}

/// Why a check or install failed, as the screen says it; the log has the details.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Failure {
    NoSource,
    /// The source couldn't be reached, or had no update.
    Download,
    /// The signature, the device or the manifest was wrong.
    NotGenuine,
    /// The U-Boot on the card can't boot slot B: it needs a flash once.
    OldBootLoader,
    /// The image didn't match its checksum, before or after writing.
    Damaged,
    Card,
    Stopped,
}

/// A check or install running in its thread.
pub struct Job {
    status: Arc<Mutex<Status>>,
    progress: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
}

impl Job {
    pub fn idle() -> Self {
        Self { status: Arc::new(Mutex::new(Status::Idle)), progress: Arc::default(), stop: Arc::default() }
    }

    /// What the job is doing; while writing or checking, with the bytes done so far.
    pub fn status(&self) -> Status {
        let status = self.status.lock().map_or(Status::Idle, |status| status.clone());
        match status {
            Status::Installing { step, total, .. } => {
                Status::Installing { step, done: self.progress.load(Ordering::Relaxed).min(total), total }
            }
            other => other,
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.status(), Status::Looking | Status::Installing { .. })
    }

    /// Stops an install between chunks; the slot it was writing stays unbootable.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Looks at `source` for an update.
    pub fn check(source: &str) -> Self {
        let job = Self::idle();
        set(&job.status, Status::Looking);
        let status = Arc::clone(&job.status);
        let source = source.to_string();
        spawn(move || {
            let result = look(&source).map(|manifest| {
                let running = slots::running_build();
                if manifest::is_newer(&manifest.build, running.as_deref()) {
                    Status::Available(manifest)
                } else {
                    Status::UpToDate(manifest.build)
                }
            });
            set(&status, result.unwrap_or_else(Status::Failed));
        });
        job
    }

    /// Installs `manifest`'s update from `source` into the other slot.
    pub fn install(source: &str, manifest: Manifest) -> Self {
        let job = Self::idle();
        set(&job.status, Status::Installing { step: Step::Writing, done: 0, total: manifest.file_size });
        let (status, progress, stop) = (Arc::clone(&job.status), Arc::clone(&job.progress), Arc::clone(&job.stop));
        let source = source.to_string();
        spawn(move || {
            let result = install(&source, &manifest, &status, &progress, &stop);
            set(&status, match result {
                Ok(()) => Status::Installed(manifest),
                Err(failure) => Status::Failed(failure),
            });
        });
        job
    }
}

fn set(status: &Mutex<Status>, value: Status) {
    if let Ok(mut status) = status.lock() {
        *status = value;
    }
}

fn spawn(work: impl FnOnce() + Send + 'static) {
    if let Err(error) = std::thread::Builder::new().name("update".into()).spawn(work) {
        eprintln!("update: could not start: {error}");
    }
}

/// A file next to the manifest at `source`, a folder's address.
pub fn file_url(source: &str, name: &str) -> String {
    format!("{}/{name}", source.trim().trim_end_matches('/'))
}

/// Plain HTTP is allowed too, as the signature is what is trusted.
fn agent(whole: Duration) -> ureq::Agent {
    http::config(whole).https_only(false).build().into()
}

fn fetch_text(agent: &ureq::Agent, url: &str) -> Result<String, Failure> {
    let response = agent.get(url).call().map_err(|error| {
        eprintln!("update: {url}: {error}");
        Failure::Download
    })?;
    response.into_body().with_config().limit(MANIFEST_LIMIT).read_to_string().map_err(|error| {
        eprintln!("update: {url}: {error}");
        Failure::Download
    })
}

/// The source's manifest, if it is genuine and for this device.
fn look(source: &str) -> Result<Manifest, Failure> {
    if source.trim().is_empty() {
        return Err(Failure::NoSource);
    }
    let agent = agent(Duration::from_secs(30));
    let text = fetch_text(&agent, &file_url(source, "manifest.txt"))?;
    let signature = fetch_text(&agent, &file_url(source, "manifest.sig"))?;
    if !manifest::verify(text.as_bytes(), &signature) {
        eprintln!("update: the manifest's signature isn't the project's");
        return Err(Failure::NotGenuine);
    }
    let manifest = manifest::parse(&text).map_err(|error| {
        eprintln!("update: {error}");
        Failure::NotGenuine
    })?;
    if !manifest.is_for(profile::get().compatible) {
        eprintln!("update: the update is for {}, not this device", manifest.devices.join(" "));
        return Err(Failure::NotGenuine);
    }
    println!("update: {} has build {} ({})", source.trim(), manifest.build, manifest.revision);
    Ok(manifest)
}

/// Counts the bytes read through it, for the progress.
struct Counting<R> {
    inner: R,
    count: Arc<AtomicU64>,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buffer)?;
        self.count.fetch_add(read as u64, Ordering::Relaxed);
        Ok(read)
    }
}

fn card_failure(what: &str) -> impl Fn(io::Error) -> Failure + '_ {
    move |error| {
        eprintln!("update: {what}: {error}");
        Failure::Card
    }
}

fn install(
    source: &str,
    manifest: &Manifest,
    status: &Mutex<Status>,
    progress: &Arc<AtomicU64>,
    stop: &AtomicBool,
) -> Result<(), Failure> {
    let running = slots::running_slot().ok_or_else(|| {
        eprintln!("update: no root slot in /proc/cmdline");
        Failure::OldBootLoader
    })?;
    let target = running.other();
    let device = slots::slot_device(target).map_err(card_failure("finding the slot"))?;
    let size = slots::partition_size(&device).map_err(card_failure("the slot's size"))?;
    if size < manifest.image_size {
        eprintln!("update: slot {} is {size} bytes, the image {}", target.name(), manifest.image_size);
        return Err(Failure::Card);
    }
    // Before anything is written: the slot is kept from booting, and U-Boot must know slots.
    let order = slots::change_env(|env| match slots::boot_order(env) {
        Some(_) => (Some(slots::while_writing(env, running)), true),
        None => (None, false),
    });
    match order {
        Ok(true) => {}
        Ok(false) => {
            eprintln!("update: U-Boot keeps no boot order: it predates A/B updates");
            return Err(Failure::OldBootLoader);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            eprintln!("update: {error}");
            return Err(Failure::OldBootLoader);
        }
        Err(error) => return Err(card_failure("the boot order")(error)),
    }
    println!("update: writing build {} to slot {} ({})", manifest.build, target.name(), device.display());

    let digest = write_image(source, manifest, &device, progress, stop)?;
    if digest != manifest.image_sha256 {
        eprintln!("update: the downloaded image doesn't match its SHA-256");
        return Err(Failure::Damaged);
    }
    set(status, Status::Installing { step: Step::Checking, done: 0, total: manifest.image_size });
    progress.store(0, Ordering::Relaxed);
    let digest = read_back(&device, manifest.image_size, progress, stop)?;
    if digest != manifest.image_sha256 {
        eprintln!("update: slot {} reads back differently from what was written", target.name());
        return Err(Failure::Damaged);
    }
    set(status, Status::Installing { step: Step::Finishing, done: 0, total: 1 });
    point_at_partition(&device, target).map_err(card_failure("the new slot's extlinux.conf"))?;
    slots::change_env(|env| (Some(slots::after_install(env, running)), ())).map_err(card_failure("the boot order"))?;
    if let Err(error) = slots::write_pending(target, &manifest.build) {
        // Only the report after the next boot misses it.
        eprintln!("warning: could not note the pending update: {error}");
    }
    let _ = Command::new("sync").status();
    println!("update: build {} is in slot {}, which starts next", manifest.build, target.name());
    Ok(())
}

/// Downloads, unpacks and writes the image to `device`. Returns the image's SHA-256.
fn write_image(
    source: &str,
    manifest: &Manifest,
    device: &Path,
    progress: &Arc<AtomicU64>,
    stop: &AtomicBool,
) -> Result<[u8; 32], Failure> {
    let url = file_url(source, &manifest.file);
    let response = agent(Duration::from_secs(3600)).get(&url).call().map_err(|error| {
        eprintln!("update: {url}: {error}");
        Failure::Download
    })?;
    // One byte over the size shows a file that is longer than the manifest says.
    let body = response.into_body().into_with_config().limit(manifest.file_size + 1).reader();
    let mut image = GzDecoder::new(Counting { inner: body, count: Arc::clone(progress) });
    let mut slot = OpenOptions::new().write(true).open(device).map_err(card_failure("opening the slot"))?;
    let mut hash = Context::new(&SHA256);
    let mut buffer = vec![0; CHUNK];
    let mut written = 0u64;
    let mut synced = 0u64;
    loop {
        if stop.load(Ordering::Relaxed) {
            println!("update: stopped after {written} bytes");
            return Err(Failure::Stopped);
        }
        let read = read_full(&mut image, &mut buffer).map_err(|error| {
            eprintln!("update: downloading or unpacking: {error}");
            Failure::Download
        })?;
        if read == 0 {
            break;
        }
        written += read as u64;
        if written > manifest.image_size {
            eprintln!("update: the image is larger than its manifest says");
            return Err(Failure::Damaged);
        }
        hash.update(&buffer[..read]);
        slot.write_all(&buffer[..read]).map_err(card_failure("writing the slot"))?;
        if written - synced >= SYNC_EVERY {
            slot.sync_data().map_err(card_failure("writing the slot"))?;
            synced = written;
        }
    }
    slot.sync_all().map_err(card_failure("writing the slot"))?;
    if written != manifest.image_size {
        eprintln!("update: the image is {written} bytes, its manifest says {}", manifest.image_size);
        return Err(Failure::Damaged);
    }
    Ok(digest_bytes(hash))
}

/// Fills `buffer` unless the data ends first; returns how much it read.
fn read_full(reader: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}

fn digest_bytes(hash: Context) -> [u8; 32] {
    let mut bytes = [0; 32];
    bytes.copy_from_slice(hash.finish().as_ref());
    bytes
}

/// The SHA-256 of the first `size` bytes of `device`, read from the card itself.
fn read_back(device: &Path, size: u64, progress: &AtomicU64, stop: &AtomicBool) -> Result<[u8; 32], Failure> {
    let mut slot = File::open(device).map_err(card_failure("opening the slot"))?;
    // What was written is still in memory: drop it, so the reads come from the card.
    // SAFETY: the descriptor is open for as long as `slot` lives, and this only gives
    // the kernel advice about its cached pages.
    let advice = unsafe { libc::posix_fadvise(slot.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED) };
    if advice != 0 {
        eprintln!("warning: could not drop the slot's cached pages: error {advice}");
    }
    let mut hash = Context::new(&SHA256);
    let mut buffer = vec![0; CHUNK];
    let mut left = size;
    while left > 0 {
        if stop.load(Ordering::Relaxed) {
            return Err(Failure::Stopped);
        }
        let want = buffer.len().min(usize::try_from(left).unwrap_or(usize::MAX));
        slot.read_exact(&mut buffer[..want]).map_err(card_failure("reading the slot back"))?;
        hash.update(&buffer[..want]);
        left -= want as u64;
        progress.fetch_add(want as u64, Ordering::Relaxed);
    }
    Ok(digest_bytes(hash))
}

/// `extlinux.conf` booting from `slot`'s partition, in place of the one it was built with,
/// with the boot messages where the running system sends them.
pub fn for_slot(conf: &str, slot: Slot, messages: Option<boot_partition::BootMessages>) -> String {
    let root = format!("root=PARTUUID=54455453-{:02}", slot.partition());
    let pointed: String = conf
        .lines()
        .map(|line| {
            let words: Vec<String> = line
                .split(' ')
                .map(|word| if word.starts_with("root=PARTUUID=") { root.clone() } else { word.to_string() })
                .collect();
            format!("{}\n", words.join(" "))
        })
        .collect();
    messages.and_then(|messages| boot_partition::with_boot_messages(&pointed, messages)).unwrap_or(pointed)
}

/// Mounts the written slot, checks it has what booting needs, and points its
/// `extlinux.conf` at its partition.
fn point_at_partition(device: &Path, slot: Slot) -> io::Result<()> {
    std::fs::create_dir_all(SLOT_MOUNT)?;
    let status = Command::new("mount").args(["-t", "ext4"]).arg(device).arg(SLOT_MOUNT).status()?;
    if !status.success() {
        return Err(io::Error::other(format!("could not mount {}: {status}", device.display())));
    }
    let root = Path::new(SLOT_MOUNT);
    let result = (|| {
        // The boot files and this device's own device tree, without which boot.cmd boots the SP's
        let device_tree = profile::get().device_tree;
        if let Some(missing) = BOOT_FILES.iter().chain([&device_tree]).find(|file| !root.join(file).is_file()) {
            return Err(io::Error::other(format!("the new slot has no {missing}")));
        }
        let path = root.join(EXTLINUX);
        let conf = std::fs::read_to_string(&path)?;
        let changed = for_slot(&conf, slot, boot_partition::boot_messages());
        let temporary = path.with_extension("new");
        std::fs::write(&temporary, &changed)?;
        File::open(&temporary)?.sync_all()?;
        std::fs::rename(&temporary, &path)?;
        println!("update: slot {} boots root=PARTUUID=54455453-{:02}", slot.name(), slot.partition());
        Ok(())
    })();
    let _ = Command::new("sync").status();
    let unmounted = Command::new("umount").arg(SLOT_MOUNT).status()?;
    result?;
    if !unmounted.success() {
        return Err(io::Error::other(format!("could not unmount {SLOT_MOUNT}: {unmounted}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::boot_partition::BootMessages;
    use flate2::write::GzEncoder;
    use flate2::Compression;

    const CONF: &str = "DEFAULT rg35xxsp\nTIMEOUT 0\n\nLABEL rg35xxsp\n  LINUX /boot/Image\n  \
        APPEND root=PARTUUID=54455453-02 rootfstype=ext4 rootwait rw console=ttynull loglevel=5 panic=10\n";

    #[test]
    fn a_slot_boots_its_own_partition_with_the_same_messages() {
        let b = for_slot(CONF, Slot::B, Some(BootMessages::Logo));
        assert_eq!(b, CONF.replace("54455453-02", "54455453-03"));
        let a = for_slot(&b, Slot::A, None);
        assert_eq!(a, CONF);
        let text = for_slot(CONF, Slot::B, Some(BootMessages::Text));
        assert!(text.contains("root=PARTUUID=54455453-03 rootfstype=ext4 rootwait rw console=tty1 loglevel=7 panic=10"));
    }

    #[test]
    fn files_are_next_to_the_manifest() {
        assert_eq!(file_url("http://10.35.35.2:8035", "manifest.txt"), "http://10.35.35.2:8035/manifest.txt");
        assert_eq!(file_url(" https://example.org/tetsu/ ", "a.img.gz"), "https://example.org/tetsu/a.img.gz");
    }

    #[test]
    fn an_image_unpacks_in_full_chunks_and_hashes_as_a_whole() {
        let image: Vec<u8> = (0..3 * CHUNK + 123).map(|at| (at % 251) as u8).collect();
        let mut packed = GzEncoder::new(Vec::new(), Compression::fast());
        packed.write_all(&image).unwrap();
        let packed = packed.finish().unwrap();
        let count = Arc::new(AtomicU64::new(0));
        let mut reader = GzDecoder::new(Counting { inner: packed.as_slice(), count: Arc::clone(&count) });
        let mut buffer = vec![0; CHUNK];
        let (mut sizes, mut hash) = (Vec::new(), Context::new(&SHA256));
        loop {
            let read = read_full(&mut reader, &mut buffer).unwrap();
            if read == 0 {
                break;
            }
            sizes.push(read);
            hash.update(&buffer[..read]);
        }
        assert_eq!(sizes, [CHUNK, CHUNK, CHUNK, 123]);
        assert_eq!(digest_bytes(hash).as_slice(), ring::digest::digest(&SHA256, &image).as_ref());
        assert_eq!(count.load(Ordering::Relaxed), packed.len() as u64, "the progress counts what was downloaded");
    }
}
