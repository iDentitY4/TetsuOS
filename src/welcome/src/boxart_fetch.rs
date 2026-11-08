//! Downloads box art over Wi-Fi, as `scripts/boxart.py` does over USB. Covers come from
//! the libretro-thumbnails collections on GitHub, which are named after No-Intro file
//! names like the games. They are fetched with curl, decoded by `png`, shrunk to the art
//! panel and saved as PPM files where `boxart::BoxArt` reads them. Games that already have
//! a cover are skipped, so running it again only fetches new games.

use crate::boxart::Image;
use crate::canvas::Rgb;
use crate::png;
use crate::storage::write_atomic;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

const RAW_URL: &str = "https://raw.githubusercontent.com/libretro-thumbnails";
const TREE_URL: &str = "https://api.github.com/repos/libretro-thumbnails";
const CA_BUNDLE: &str = "/etc/ssl/certs/ca-certificates.crt";
/// The art panel in the launcher's game lists.
pub const MAX_SIZE: (usize, usize) = (200, 300);
/// How many symbolic link stubs are followed from one name.
const MAX_LINKS: usize = 3;

/// A game to find a cover for.
#[derive(Clone, Debug)]
pub struct Job {
    pub rom: PathBuf,
    /// Its console's collection, such as `Nintendo_-_Game_Boy`.
    pub collection: &'static str,
}

/// What the download has got to, sent as it goes.
#[derive(Clone, PartialEq, Debug)]
pub enum Progress {
    /// Looking for the cover of game `done + 1` of `total`.
    Fetching { done: usize, total: usize, name: String },
    Finished(Summary),
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Summary {
    pub found: usize,
    /// Games with no cover in their collection, by file name without the extension.
    pub missing: Vec<String>,
    /// Why it stopped early, if it did.
    pub error: Option<String>,
    pub stopped: bool,
}

/// Where downloads come from: GitHub through curl, or a test's files.
pub trait Source {
    /// The file at `url`, or None if the server has no such file.
    fn get(&self, url: &str) -> Result<Option<Vec<u8>>, String>;
}

pub struct Curl;

impl Source for Curl {
    fn get(&self, url: &str) -> Result<Option<Vec<u8>>, String> {
        let output = Command::new("curl")
            .args(["--fail", "--silent", "--show-error", "--location", "--proto", "=https"])
            .args(["--connect-timeout", "15", "--max-time", "60", "--cacert", CA_BUNDLE])
            .arg(url)
            .output()
            .map_err(|error| {
                eprintln!("boxart: could not run curl: {error}");
                "CURL IS MISSING".to_string()
            })?;
        match output.status.code() {
            Some(0) => Ok(Some(output.stdout)),
            // The server answered with an error, which for a cover means there is none.
            Some(22) => Ok(None),
            code => {
                let message = String::from_utf8_lossy(&output.stderr);
                eprintln!("boxart: curl failed ({code:?}) for {url}: {}", message.trim());
                Err(match code {
                    Some(6 | 7 | 28) => "NO CONNECTION TO GITHUB".into(),
                    Some(35 | 60) => "SECURE CONNECTION FAILED - CHECK THE DATE".into(),
                    _ => "DOWNLOAD FAILED".into(),
                })
            }
        }
    }
}

/// libretro-thumbnails replaces characters that file systems disallow.
pub fn thumbnail_name(stem: &str) -> String {
    stem.chars().map(|c| if "&*/:`<>?\\|\"".contains(c) { '_' } else { c }).collect()
}

/// Percent-encodes a URL path segment, keeping only unreserved characters.
fn quote(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn cover_url(collection: &str, name: &str) -> String {
    format!("{RAW_URL}/{collection}/master/Named_Boxarts/{}.png", quote(name))
}

const PNG_SIGNATURE: &[u8] = b"\x89PNG";

/// The cover's PNG bytes, or None if there is no cover by that name. The collections hold
/// alternative names as symbolic links, which GitHub serves as a file holding the
/// target's name, so those are followed.
fn fetch_cover(source: &dyn Source, collection: &str, name: &str) -> Result<Option<Vec<u8>>, String> {
    let mut data = source.get(&cover_url(collection, name))?;
    for _ in 0..MAX_LINKS {
        let Some(bytes) = &data else { break };
        if bytes.starts_with(PNG_SIGNATURE) {
            break;
        }
        let target = String::from_utf8_lossy(bytes).trim().to_string();
        let Some(stem) = target.strip_suffix(".png").filter(|stem| !stem.contains('/')) else {
            return Ok(None);
        };
        data = source.get(&cover_url(collection, stem))?;
    }
    Ok(data.filter(|bytes| bytes.starts_with(PNG_SIGNATURE)))
}

/// Every cover name in a collection, from GitHub's listing of its files. Empty if the
/// listing can't be had, such as past GitHub's limit of 60 listings an hour.
fn cover_names(source: &dyn Source, collection: &str) -> Result<Vec<String>, String> {
    let url = format!("{TREE_URL}/{collection}/git/trees/master?recursive=1");
    let Some(listing) = source.get(&url)? else {
        eprintln!("boxart: no file listing for {collection}");
        return Ok(Vec::new());
    };
    Ok(json_strings_after(&String::from_utf8_lossy(&listing), "\"path\"")
        .into_iter()
        .filter_map(|path| path.strip_prefix("Named_Boxarts/")?.strip_suffix(".png").map(String::from))
        .collect())
}

/// The JSON string values that follow each occurrence of `key`, such as every `"path"`.
/// Enough of JSON for GitHub's listing, without a parser.
fn json_strings_after(json: &str, key: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut rest = json;
    while let Some(found) = rest.find(key) {
        rest = &rest[found + key.len()..];
        let Some(after_colon) = rest.trim_start().strip_prefix(':') else { continue };
        let Some(body) = after_colon.trim_start().strip_prefix('"') else { continue };
        let (value, remaining) = json_string(body);
        values.push(value);
        rest = remaining;
    }
    values
}

/// Reads a JSON string body up to its closing quote. Returns it and what follows.
fn json_string(body: &str) -> (String, &str) {
    let mut value = String::new();
    let mut chars = body.char_indices();
    let mut pending_high: Option<u32> = None;
    while let Some((index, c)) = chars.next() {
        match c {
            '"' => return (value, &body[index + 1..]),
            '\\' => {
                let Some((_, escape)) = chars.next() else { break };
                match escape {
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    'r' => value.push('\r'),
                    'b' => value.push('\u{8}'),
                    'f' => value.push('\u{c}'),
                    'u' => {
                        let hex: String = chars.by_ref().take(4).map(|(_, c)| c).collect();
                        let Ok(unit) = u32::from_str_radix(&hex, 16) else { continue };
                        if (0xD800..0xDC00).contains(&unit) {
                            pending_high = Some(unit);
                            continue;
                        }
                        let code = match pending_high.take() {
                            Some(high) if (0xDC00..0xE000).contains(&unit) => {
                                0x10000 + ((high - 0xD800) << 10) + (unit - 0xDC00)
                            }
                            _ => unit,
                        };
                        value.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                    }
                    other => value.push(other),
                }
            }
            _ => value.push(c),
        }
    }
    (value, "")
}

/// Splits "Tetris DX (World) (SGB Enhanced)" into "tetris dx" and its tags.
fn title_and_tags(name: &str) -> (String, Vec<String>) {
    let title = name.split(" (").next().unwrap_or_default().to_lowercase();
    let tags = name
        .split('(')
        .skip(1)
        .filter_map(|part| part.split_once(')').map(|(tag, _)| tag.to_lowercase()))
        .collect();
    (title, tags)
}

/// The cover with the same title, preferring the most tags in common, such as region,
/// and the fewest others. The first of equals wins, as in the script.
fn closest_cover<'a>(names: &'a [String], stem: &str) -> Option<&'a str> {
    let (title, tags) = title_and_tags(stem);
    let mut best: Option<(&str, i64)> = None;
    for name in names {
        let (other_title, other_tags) = title_and_tags(name);
        if other_title != title {
            continue;
        }
        let shared = other_tags.iter().filter(|tag| tags.contains(tag)).count() as i64;
        let extra = other_tags.iter().filter(|tag| !tags.contains(tag)).count() as i64;
        let score = shared * 2 - extra;
        if best.is_none_or(|(_, best_score)| score > best_score) {
            best = Some((name, score));
        }
    }
    best.map(|(name, _)| name)
}

/// The size that fits `max` with the same shape, never larger than the image.
fn fit_size((width, height): (usize, usize), (max_w, max_h): (usize, usize)) -> (usize, usize) {
    if width <= max_w && height <= max_h {
        return (width, height);
    }
    if width * max_h >= height * max_w {
        (max_w, ((height * max_w + width / 2) / width).max(1))
    } else {
        (((width * max_h + height / 2) / height).max(1), max_h)
    }
}

/// Shrinks an image to fit `max` by averaging the pixels each new one covers, with
/// partial pixels at the edges weighted by how much of them it covers.
pub fn shrink(image: &Image, max: (usize, usize)) -> Image {
    let (width, height) = fit_size((image.width, image.height), max);
    if (width, height) == (image.width, image.height) {
        return Image { width, height, pixels: image.pixels.clone() };
    }
    let columns = box_weights(image.width, width);
    let rows = box_weights(image.height, height);
    let mut pixels = Vec::with_capacity(width * height);
    for row in &rows {
        for column in &columns {
            let mut sum = [0u64; 3];
            let mut total = 0u64;
            for &(y, wy) in row {
                for &(x, wx) in column {
                    let weight = u64::from(wy) * u64::from(wx);
                    let Rgb(r, g, b) = image.pixels[y * image.width + x];
                    sum[0] += u64::from(r) * weight;
                    sum[1] += u64::from(g) * weight;
                    sum[2] += u64::from(b) * weight;
                    total += weight;
                }
            }
            let channel = |value: u64| ((value + total / 2) / total) as u8;
            pixels.push(Rgb(channel(sum[0]), channel(sum[1]), channel(sum[2])));
        }
    }
    Image { width, height, pixels }
}

/// For each of `dest` pixels along an axis, the source pixels it covers, with how much of
/// each, in units of 1/dest of a source pixel.
fn box_weights(source: usize, dest: usize) -> Vec<Vec<(usize, u32)>> {
    (0..dest)
        .map(|index| {
            let (start, end) = (index * source, (index + 1) * source);
            (start / dest..end.div_ceil(dest))
                .map(|pixel| {
                    let covered = end.min((pixel + 1) * dest) - start.max(pixel * dest);
                    (pixel, covered as u32)
                })
                .collect()
        })
        .collect()
}

pub fn ppm_bytes(image: &Image) -> Vec<u8> {
    let mut bytes = format!("P6\n{} {}\n255\n", image.width, image.height).into_bytes();
    bytes.extend(image.pixels.iter().flat_map(|&Rgb(r, g, b)| [r, g, b]));
    bytes
}

/// The cover file for a game, named after its file without the extension.
pub fn art_path(directory: &Path, rom: &Path) -> PathBuf {
    let mut name = rom.file_stem().unwrap_or_default().to_os_string();
    name.push(".ppm");
    directory.join(name)
}

/// Fetches covers for each job in turn, sending progress, until done or `stop` is set.
pub fn run(source: &dyn Source, jobs: &[Job], directory: &Path, stop: &AtomicBool, progress: &Sender<Progress>) -> Summary {
    let mut summary = Summary::default();
    let mut listings: HashMap<&str, Vec<String>> = HashMap::new();
    for (done, job) in jobs.iter().enumerate() {
        if stop.load(Ordering::Relaxed) {
            summary.stopped = true;
            break;
        }
        let stem = job.rom.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let _ = progress.send(Progress::Fetching { done, total: jobs.len(), name: stem.clone() });
        match fetch_one(source, job, &stem, &mut listings) {
            Ok(Some(png)) => {
                let saved = png::decode(&png)
                    .map_err(|error| format!("bad image: {error}"))
                    .and_then(|image| {
                        let path = art_path(directory, &job.rom);
                        write_atomic(&path, &ppm_bytes(&shrink(&image, MAX_SIZE))).map_err(|error| error.to_string())
                    });
                match saved {
                    Ok(()) => summary.found += 1,
                    Err(error) => {
                        eprintln!("boxart: {stem}: {error}");
                        summary.missing.push(stem);
                    }
                }
            }
            Ok(None) => summary.missing.push(stem),
            Err(error) => {
                summary.error = Some(error);
                break;
            }
        }
    }
    summary
}

fn fetch_one<'a>(
    source: &dyn Source,
    job: &'a Job,
    stem: &str,
    listings: &mut HashMap<&'a str, Vec<String>>,
) -> Result<Option<Vec<u8>>, String> {
    if let Some(png) = fetch_cover(source, job.collection, &thumbnail_name(stem))? {
        return Ok(Some(png));
    }
    // Names differ in tags between sets, like (GB Compatible); try the closest.
    if !listings.contains_key(job.collection) {
        let names = cover_names(source, job.collection)?;
        listings.insert(job.collection, names);
    }
    match closest_cover(&listings[job.collection], stem) {
        Some(name) => fetch_cover(source, job.collection, &name.to_string()),
        None => Ok(None),
    }
}

/// Runs the download on its own thread. Set `stop` to end it after the current game.
pub fn spawn(jobs: Vec<Job>, directory: PathBuf, stop: Arc<AtomicBool>) -> Receiver<Progress> {
    let (sender, receiver) = channel();
    std::thread::spawn(move || {
        if let Err(error) = std::fs::create_dir_all(&directory) {
            eprintln!("boxart: could not make {}: {error}", directory.display());
        }
        let summary = run(&Curl, &jobs, &directory, &stop, &sender);
        println!("boxart: {} covers saved, {} not found", summary.found, summary.missing.len());
        let _ = sender.send(Progress::Finished(summary));
    });
    receiver
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Serves files by URL, and records what was asked for.
    struct Files {
        files: HashMap<String, Vec<u8>>,
        asked: RefCell<Vec<String>>,
        fail: bool,
    }

    impl Source for Files {
        fn get(&self, url: &str) -> Result<Option<Vec<u8>>, String> {
            self.asked.borrow_mut().push(url.to_string());
            if self.fail {
                return Err("NO CONNECTION TO GITHUB".into());
            }
            Ok(self.files.get(url).cloned())
        }
    }

    fn test_png() -> Vec<u8> {
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/rgb.png")).unwrap()
    }

    #[test]
    fn names_are_made_safe_and_quoted() {
        assert_eq!(thumbnail_name("Ren & Stimpy: \"Yes\"?"), "Ren _ Stimpy_ _Yes__");
        assert_eq!(quote("Pokemon - Red (USA, Europe)"), "Pokemon%20-%20Red%20%28USA%2C%20Europe%29");
        assert_eq!(quote("Pokémon"), "Pok%C3%A9mon");
    }

    #[test]
    fn link_stubs_are_followed() {
        let collection = "Nintendo_-_Game_Boy";
        let mut files = HashMap::new();
        files.insert(cover_url(collection, "Alias"), b"Real Name.png\n".to_vec());
        files.insert(cover_url(collection, "Real Name"), test_png());
        files.insert(cover_url(collection, "Loop"), b"Loop.png".to_vec());
        files.insert(cover_url(collection, "Bad"), b"../escape.png".to_vec());
        let source = Files { files, asked: RefCell::default(), fail: false };
        assert_eq!(fetch_cover(&source, collection, "Alias").unwrap(), Some(test_png()));
        assert_eq!(fetch_cover(&source, collection, "Loop").unwrap(), None);
        assert_eq!(fetch_cover(&source, collection, "Bad").unwrap(), None);
        assert_eq!(fetch_cover(&source, collection, "Missing").unwrap(), None);
    }

    #[test]
    fn the_listing_gives_cover_names() {
        let json = r#"{"sha":"x","tree":[{"path":"Named_Boxarts","type":"tree"},
            {"path" : "Named_Boxarts/Tetris DX (World) (SGB Enhanced) (GB Compatible).png"},
            {"path":"Named_Snaps/Tetris.png"},
            {"path":"Named_Boxarts/Pokémon \"Q\" 😀.png"}],"truncated":false}"#;
        let mut files = HashMap::new();
        files.insert(format!("{TREE_URL}/C/git/trees/master?recursive=1"), json.as_bytes().to_vec());
        let source = Files { files, asked: RefCell::default(), fail: false };
        assert_eq!(
            cover_names(&source, "C").unwrap(),
            ["Tetris DX (World) (SGB Enhanced) (GB Compatible)", "Pokémon \"Q\" 😀"]
        );
    }

    #[test]
    fn the_closest_name_shares_the_title_and_most_tags() {
        let names: Vec<String> = [
            "Tetris (World)",
            "Tetris DX (Japan)",
            "Tetris DX (World) (SGB Enhanced) (GB Compatible)",
            "Tetris DX (World) (Beta) (Proto) (Demo)",
        ]
        .map(String::from)
        .into();
        assert_eq!(closest_cover(&names, "Tetris DX (World) (SGB Enhanced)"), Some(names[2].as_str()));
        assert_eq!(closest_cover(&names, "Tetris DX (Japan)"), Some(names[1].as_str()));
        assert_eq!(closest_cover(&names, "Tetris Attack (USA)"), None);
    }

    #[test]
    fn shrinking_keeps_the_shape_and_averages() {
        assert_eq!(fit_size((512, 512), MAX_SIZE), (200, 200));
        assert_eq!(fit_size((512, 724), MAX_SIZE), (200, 283));
        assert_eq!(fit_size((400, 1000), MAX_SIZE), (120, 300));
        assert_eq!(fit_size((100, 80), MAX_SIZE), (100, 80));
        // Two by one to one pixel: the average of both.
        let image = Image { width: 4, height: 2, pixels: vec![Rgb(0, 0, 0), Rgb(200, 100, 50), Rgb(10, 10, 10), Rgb(10, 10, 10)].repeat(2) };
        let small = shrink(&image, (2, 1));
        assert_eq!((small.width, small.height), (2, 1));
        assert_eq!(small.pixels, [Rgb(100, 50, 25), Rgb(10, 10, 10)]);
        // Uneven: 3 pixels into 2, each covering one and a half.
        let image = Image { width: 3, height: 1, pixels: vec![Rgb(90, 0, 0), Rgb(0, 0, 0), Rgb(0, 0, 90)] };
        assert_eq!(shrink(&image, (2, 1)).pixels, [Rgb(60, 0, 0), Rgb(0, 0, 60)]);
    }

    #[test]
    fn a_run_saves_covers_and_lists_the_rest() {
        let directory = std::env::temp_dir().join(format!("welcome-fetch-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let collection = "Nintendo_-_Game_Boy";
        let mut files = HashMap::new();
        files.insert(cover_url(collection, "Found (USA)"), test_png());
        files.insert(cover_url(collection, "Close (World) (GB Compatible)"), test_png());
        let listing = r#"{"tree":[{"path":"Named_Boxarts/Close (World) (GB Compatible).png"}]}"#;
        files.insert(format!("{TREE_URL}/{collection}/git/trees/master?recursive=1"), listing.into());
        let source = Files { files, asked: RefCell::default(), fail: false };
        let jobs: Vec<Job> = ["Found (USA).gb", "Close (World).gb", "Gone (USA).gb"]
            .iter()
            .map(|name| Job { rom: PathBuf::from("/roms").join(name), collection })
            .collect();
        let (sender, receiver) = channel();
        let summary = run(&source, &jobs, &directory, &AtomicBool::new(false), &sender);
        assert_eq!(summary, Summary { found: 2, missing: vec!["Gone (USA)".into()], error: None, stopped: false });
        let saved = std::fs::read(directory.join("Close (World).ppm")).unwrap();
        let image = crate::boxart::parse_ppm(&saved).unwrap();
        assert_eq!((image.width, image.height), (37, 23));
        assert!(directory.join("Found (USA).ppm").exists());
        // The listing was fetched once, for both games that needed it.
        let listings = source.asked.borrow().iter().filter(|url| url.contains("/git/trees/")).count();
        assert_eq!(listings, 1);
        let sent: Vec<Progress> = receiver.try_iter().collect();
        assert_eq!(sent[2], Progress::Fetching { done: 2, total: 3, name: "Gone (USA)".into() });
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_failure_or_stop_ends_the_run() {
        let jobs = vec![Job { rom: "/roms/A.gb".into(), collection: "C" }; 3];
        let (sender, _receiver) = channel();
        let failing = Files { files: HashMap::new(), asked: RefCell::default(), fail: true };
        let summary = run(&failing, &jobs, Path::new("/nonexistent"), &AtomicBool::new(false), &sender);
        assert_eq!(summary.error.as_deref(), Some("NO CONNECTION TO GITHUB"));
        assert_eq!(failing.asked.borrow().len(), 1);
        let source = Files { files: HashMap::new(), asked: RefCell::default(), fail: false };
        let summary = run(&source, &jobs, Path::new("/nonexistent"), &AtomicBool::new(true), &sender);
        assert!(summary.stopped && source.asked.borrow().is_empty());
    }
}
