//! The web page of Settings > SYSTEM > WI-FI TRANSFER, which runs only while that screen is
//! open: a phone or PC browser on the same network sends games, saves and covers, downloads
//! saves and screenshots, and deletes games. The browser needs the code on the screen
//! once, after which a cookie lets it in until the screen closes. Changes need the
//! `X-Tetsu` header too, which another site's page can't send, and wrong codes are
//! answered one a second, so guessing the six digits would take days.
//!
//! The page itself is `transfer_page.html`; what it asks for is answered here, each request
//! on a thread of its own. Uploads go to a hidden file beside where they belong and are
//! renamed into place once complete, so the games never show half a file.
//!
//! A game is named by its path in `roms`, and one on a card in TF2 by that path after
//! `TF2:`, which no file on FAT32 or exFAT can start with. Uploads go to TETSU-DATA, or to
//! TF2 with `card=tf2`.

use crate::library::boxart::{parse_ppm, ART_DIR};
use crate::library::boxart_fetch::{art_path, MAX_SIZE};
use crate::library::game_files::{delete_game, GameFiles};
use crate::library::cards::{self, TF2_ROM_DIR};
use crate::library::{build_library_with, find_all_roms, find_roms, game_title, rom_label, Lists};
use crate::libretro::resume_path;
use crate::ingame::screenshot::SCREENSHOT_DIR;
use crate::device::storage::write_atomic;
use crate::library::systems::{save_path, system_for, system_for_extension, system_index, System, ROM_DIR, SAVE_DIR, SYSTEMS};
use crate::ui::color::Rgb;
use crate::library::rom_zip;
use crate::ui::icons;
use crate::settings;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, ResponseBox};

const PAGE: &str = include_str!("transfer_page.html");
const ADDRESS: &str = "0.0.0.0:80";
const COOKIE: &str = "tetsu";
/// The header every change needs, which a form or link on another site can't send.
const CHANGE_HEADER: &str = "X-Tetsu";
/// How long a wrong code takes to answer. Guesses wait for each other.
const WRONG_CODE_WAIT: Duration = Duration::from_secs(1);
/// What starts the hidden name an upload has until it is complete.
const UPLOAD_PREFIX: &str = ".upload-";
/// How long stopping waits for requests under way, before leaving them to finish alone.
const HANDLER_WAIT: Duration = Duration::from_secs(2);
/// Numbers each upload's hidden name, so two uploads of one name never share a file.
static UPLOAD_NUMBER: AtomicU64 = AtomicU64::new(0);
/// The largest file FAT32 holds.
const LARGEST_FILE: u64 = 0xFFFF_FFFF;
/// What the card keeps free, for saves and settings.
const KEPT_FREE: u64 = 16 << 20;
/// The largest save taken, more than any console's memory or state.
const LARGEST_SAVE: u64 = 64 << 20;
/// The files beside a `PlayStation` `.cue`, which aren't games of their own.
const DISC_TRACKS: &[&str] = &["bin", "img", "iso", "wav", "sub", "sbi"];
const DISC_CONSOLE: &str = "ps1";

/// What has just happened, for the screen.
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Done {
    Received(String),
    Failed(String),
    SaveReplaced(String),
    CoverSaved(String),
    Deleted(String),
    /// A backup ZIP was restored, with this many files.
    Restored(usize),
}

/// An upload under way.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Receiving {
    pub(crate) name: String,
    pub(crate) received: u64,
    pub(crate) size: u64,
}

impl Receiving {
    pub(crate) fn percent(&self) -> u64 {
        (self.received * 100).checked_div(self.size).unwrap_or(100)
    }
}

/// What the screen shows of the server's work.
#[derive(Clone, PartialEq, Debug, Default)]
pub(crate) struct Activity {
    pub(crate) receiving: Option<Receiving>,
    pub(crate) last: Option<Done>,
    /// Whether a browser has given the code.
    pub(crate) browser: bool,
    /// Whether games, saves or covers changed, so the launcher reads them again.
    pub(crate) changed: bool,
    /// Whether a backup was restored, which replaces the settings and lists the launcher
    /// holds, so it starts again once the screen closes.
    pub(crate) restored: bool,
}

struct Shared {
    code: String,
    sessions: Mutex<Vec<String>>,
    guesses: Mutex<()>,
    stop: AtomicBool,
    activity: Mutex<Activity>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Shared {
    fn update(&self, change: impl FnOnce(&mut Activity)) {
        change(&mut lock(&self.activity));
    }

    fn finished(&self, done: Done, changed: bool) {
        self.update(|activity| {
            activity.last = Some(done);
            activity.changed |= changed;
        });
    }
}

/// The running server. Stopping it, or dropping it, stops uploads under way and waits for
/// up to `HANDLER_WAIT` for each request's thread.
pub(crate) struct Server {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    /// Starts listening on every address, for those who give `code`.
    pub(crate) fn start(code: &str) -> io::Result<Self> {
        let server = tiny_http::Server::http(ADDRESS).map_err(io::Error::other)?;
        for folder in cards::rom_dirs() {
            remove_unfinished(folder);
        }
        // A backup ZIP being restored waits at TETSU-DATA's top level.
        for entry in std::fs::read_dir(crate::device::paths::DATA_DIR).into_iter().flatten().flatten() {
            if entry.file_name().to_string_lossy().starts_with(UPLOAD_PREFIX) && entry.file_type().is_ok_and(|kind| kind.is_file()) {
                println!("transfer: removing the unfinished {}", entry.path().display());
                let _ = std::fs::remove_file(entry.path());
            }
        }
        let shared = Arc::new(Shared {
            code: code.to_string(),
            sessions: Mutex::default(),
            guesses: Mutex::default(),
            stop: AtomicBool::new(false),
            activity: Mutex::default(),
        });
        let serving = Arc::clone(&shared);
        let thread = thread::Builder::new().name("transfer".into()).spawn(move || serve(&server, &serving))?;
        Ok(Self { shared, thread: Some(thread) })
    }

    pub(crate) fn activity(&self) -> Activity {
        lock(&self.shared.activity).clone()
    }

    /// Stops, and returns whether anything on the card changed.
    pub(crate) fn stop(mut self) -> bool {
        self.shut_down();
        self.activity().changed
    }

    fn shut_down(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                eprintln!("warning: the transfer server's thread failed");
            }
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.shut_down();
    }
}

fn serve(server: &tiny_http::Server, shared: &Arc<Shared>) {
    let mut handlers: Vec<JoinHandle<()>> = Vec::new();
    while !shared.stop.load(Ordering::Relaxed) {
        match server.recv_timeout(Duration::from_millis(200)) {
            Ok(Some(request)) => {
                let shared = Arc::clone(shared);
                match thread::Builder::new().name("transfer-request".into()).spawn(move || handle(request, &shared)) {
                    Ok(handler) => handlers.push(handler),
                    Err(error) => eprintln!("warning: no thread for a transfer request: {error}"),
                }
            }
            Ok(None) => {}
            Err(error) => {
                eprintln!("warning: the transfer server stopped: {error}");
                break;
            }
        }
        handlers.retain(|handler| !handler.is_finished());
    }
    // Uploads see the stop between reads, but a read from a phone that left the network
    // never returns, so those still waiting after a while are left to end alone.
    let deadline = Instant::now() + HANDLER_WAIT;
    while handlers.iter().any(|handler| !handler.is_finished()) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    let (finished, waiting): (Vec<_>, Vec<_>) = handlers.into_iter().partition(JoinHandle::is_finished);
    for handler in finished {
        let _ = handler.join();
    }
    if !waiting.is_empty() {
        eprintln!("warning: left {} transfer requests still running", waiting.len());
    }
}

/// The hidden name an upload of `name` has until it is complete: unique to this upload, and
/// ending in `name`, so its extension still tells a ZIP.
fn upload_name(name: &str) -> String {
    let number = UPLOAD_NUMBER.fetch_add(1, Ordering::Relaxed);
    format!("{UPLOAD_PREFIX}{number}-{name}")
}

/// A refusal, with a reason the page shows in its own words.
#[derive(Debug, PartialEq)]
struct Refused {
    status: u16,
    reason: &'static str,
}

const fn refused(status: u16, reason: &'static str) -> Refused {
    Refused { status, reason }
}

const NOT_LOGGED_IN: Refused = refused(401, "login");
const NO_CHANGE_HEADER: Refused = refused(403, "header");
const NOT_FOUND: Refused = refused(404, "not-found");
const BAD_NAME: Refused = refused(400, "bad-name");
const NOT_A_GAME: Refused = refused(415, "not-a-game");
const NO_GAME: Refused = refused(404, "no-game");
const NO_CARD: Refused = refused(404, "no-card");
const NO_LENGTH: Refused = refused(411, "no-length");
const TOO_LARGE: Refused = refused(413, "too-large");
const NO_ROOM: Refused = refused(507, "no-room");
const NOT_A_COVER: Refused = refused(415, "not-a-cover");
const INCOMPLETE: Refused = refused(400, "incomplete");
const NOT_WRITTEN: Refused = refused(500, "write-failed");
const NOT_A_BACKUP: Refused = refused(415, "not-a-backup");

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap_or_else(|()| {
        Header::from_bytes(&b"X-Invalid"[..], &b"1"[..]).expect("a plain header is valid")
    })
}

fn json(value: &impl Serialize) -> ResponseBox {
    let body = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    Response::from_string(body).with_header(header("Content-Type", "application/json")).boxed()
}

fn refusal(refused: &Refused) -> ResponseBox {
    // The language, for the page to ask for the code in.
    let body = serde_json::json!({ "error": refused.reason, "language": settings::language().key() }).to_string();
    Response::from_string(body)
        .with_status_code(refused.status)
        .with_header(header("Content-Type", "application/json"))
        .boxed()
}

fn redirect(to: &str, cookie: Option<&str>) -> ResponseBox {
    let mut response = Response::empty(303).with_header(header("Location", to));
    if let Some(token) = cookie {
        response.add_header(header("Set-Cookie", &format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax")));
    }
    response.boxed()
}

fn handle(mut request: Request, shared: &Shared) {
    let reply = route(&mut request, shared).unwrap_or_else(|refused| refusal(&refused));
    if let Err(error) = request.respond(reply) {
        eprintln!("transfer: could not answer: {error}");
    }
}

fn header_value<'a>(request: &'a Request, name: &'static str) -> Option<&'a str> {
    request.headers().iter().find(|header| header.field.equiv(name)).map(|header| header.value.as_str())
}

fn route(request: &mut Request, shared: &Shared) -> Result<ResponseBox, Refused> {
    let (path, query) = split_url(request.url());
    let method = request.method().clone();
    if method == Method::Get && path == "/logo.svg" {
        return Ok(Response::from_string(logo_svg())
            .with_header(header("Content-Type", "image/svg+xml"))
            .with_header(header("Cache-Control", "max-age=3600"))
            .boxed());
    }
    if method == Method::Get && path == "/" {
        return Ok(match query.get("code") {
            Some(code) => log_in(code, shared),
            None => Response::from_string(PAGE)
                .with_header(header("Content-Type", "text/html; charset=utf-8"))
                .with_header(header("Cache-Control", "no-store"))
                .boxed(),
        });
    }
    let cookies = header_value(request, "Cookie").unwrap_or_default();
    if !cookie_token(cookies).is_some_and(|token| lock(&shared.sessions).iter().any(|known| known == token)) {
        return Err(NOT_LOGGED_IN);
    }
    if method != Method::Get && header_value(request, CHANGE_HEADER).is_none() {
        return Err(NO_CHANGE_HEADER);
    }
    let argument = |name: &str| query.get(name).map(String::as_str).unwrap_or_default();
    match (method, path.as_str()) {
        (Method::Get, "/api/list") => Ok(json(&listing())),
        (Method::Get, "/api/cover") => cover_image(&find_game(argument("game"))?),
        (Method::Get, "/api/file") => download(argument("path")),
        (Method::Get, "/api/backup") => download_backup(),
        (Method::Put, "/api/restore") => restore_backup(request, shared),
        (Method::Put, "/api/game") => upload_game(request, argument("name"), card_folder(argument("card"))?, shared),
        (Method::Put, "/api/save") => upload_save(request, &find_game(argument("game"))?, shared),
        (Method::Put, "/api/cover") => upload_cover(request, &find_game(argument("game"))?, shared),
        (Method::Delete, "/api/game") => remove_game(&find_game(argument("game"))?, shared),
        _ => Err(NOT_FOUND),
    }
}

/// Checks a code, from the QR code's address or the page's form. Right, it sets the cookie
/// and goes to the page without the code in the address; wrong, the page says so.
fn log_in(code: &str, shared: &Shared) -> ResponseBox {
    let digits: String = code.chars().filter(char::is_ascii_digit).collect();
    let _turn = lock(&shared.guesses);
    if digits != shared.code {
        println!("transfer: a wrong code");
        thread::sleep(WRONG_CODE_WAIT);
        return redirect("/?wrong", None);
    }
    let token = match random_bytes::<16>() {
        Ok(bytes) => bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
        Err(error) => {
            eprintln!("warning: no random numbers for a transfer session: {error}");
            return redirect("/?wrong", None);
        }
    };
    lock(&shared.sessions).push(token.clone());
    shared.update(|activity| activity.browser = true);
    println!("transfer: a browser gave the code");
    redirect("/", Some(&token))
}

/// Random bytes from the kernel.
pub(crate) fn random_bytes<const N: usize>() -> io::Result<[u8; N]> {
    let mut bytes = [0; N];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes)
}

/// The path and the arguments of an address such as `/api/game?name=A%20B.gba`.
fn split_url(url: &str) -> (String, BTreeMap<String, String>) {
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    let arguments = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter_map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            Some((decode(name)?, decode(value)?))
        })
        .collect();
    (decode(path).unwrap_or_default(), arguments)
}

/// Undoes the `%XX` of an address, and a form's `+` for a space.
fn decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let hex = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
                decoded.push(u8::from_str_radix(hex, 16).ok()?);
                index += 3;
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).ok()
}

/// The session in a `Cookie` header.
fn cookie_token(cookies: &str) -> Option<&str> {
    cookies.split(';').filter_map(|pair| pair.trim().split_once('=')).find(|(name, _)| *name == COOKIE).map(|(_, value)| value)
}

/// Whether a file name from the browser is one name the card can hold, not a path.
fn valid_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('.')
        && name.trim() == name
        && !name.chars().any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
}

fn extension_of(name: &str) -> String {
    Path::new(name).extension().map(|extension| extension.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// The console a file goes to, by its extension. A ZIP's is known once it has arrived.
fn console_for_upload(extension: &str) -> Option<&'static System> {
    if DISC_TRACKS.contains(&extension) {
        return SYSTEMS.iter().find(|system| system.key == DISC_CONSOLE);
    }
    system_for_extension(extension)
}

/// Where a console's new games go: the folder in `roms` that most of its games are in,
/// counting a game in a folder of its own as being in the one above, or a new one named
/// after the console.
fn folder_for(system: &System, roms: &[PathBuf], root: &Path) -> PathBuf {
    let mut counts: BTreeMap<PathBuf, usize> = BTreeMap::new();
    for rom in roms.iter().filter(|rom| system_for(rom).is_some_and(|other| other.key == system.key)) {
        if let Ok(relative) = rom.strip_prefix(root) {
            let mut components = relative.components();
            if let (Some(Component::Normal(first)), Some(_)) = (components.next(), components.next()) {
                if !first.to_string_lossy().starts_with('.') {
                    *counts.entry(PathBuf::from(first)).or_default() += 1;
                }
            }
        }
    }
    // The most games, and of those the first by name.
    let best = counts.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0))).map(|(folder, _)| folder.clone());
    root.join(best.unwrap_or_else(|| PathBuf::from(system.key.to_uppercase())))
}

/// Removes what uploads cut off by a power cut left behind.
fn remove_unfinished(folder: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            remove_unfinished(&path);
        } else if entry.file_name().to_string_lossy().starts_with(UPLOAD_PREFIX) {
            println!("transfer: removing the unfinished {}", path.display());
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn free_space(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: `name` is a valid C string and `stats` is written by the call.
    unsafe {
        let mut stats: libc::statvfs = std::mem::zeroed();
        (libc::statvfs(name.as_ptr(), &raw mut stats) == 0).then(|| stats.f_bavail * stats.f_frsize)
    }
}

fn sync_folder(folder: &Path) -> io::Result<()> {
    File::open(folder)?.sync_all()
}

/// Writes the request's body to `path`, removing it unless it all arrived.
fn receive(body: &mut dyn Read, path: &Path, size: u64, name: &str, shared: &Shared) -> Result<(), Refused> {
    shared.update(|activity| activity.receiving = Some(Receiving { name: name.to_string(), received: 0, size }));
    let result = (|| -> io::Result<bool> {
        let mut file = File::create(path)?;
        let mut buffer = vec![0; 256 << 10];
        let mut received = 0;
        while received < size {
            if shared.stop.load(Ordering::Relaxed) {
                return Ok(false);
            }
            let read = body.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read])?;
            received += read as u64;
            shared.update(|activity| {
                if let Some(receiving) = &mut activity.receiving {
                    receiving.received = received;
                }
            });
        }
        file.sync_all()?;
        Ok(received == size)
    })();
    shared.update(|activity| activity.receiving = None);
    let refused = match result {
        Ok(true) => return Ok(()),
        Ok(false) => INCOMPLETE,
        Err(error) => {
            eprintln!("transfer: could not write {}: {error}", path.display());
            NOT_WRITTEN
        }
    };
    let _ = std::fs::remove_file(path);
    shared.finished(Done::Failed(name.to_string()), false);
    Err(refused)
}

#[derive(Serialize)]
struct Uploaded {
    path: String,
    replaced: bool,
}

/// The `roms` folder of the card an upload asks for, TETSU-DATA's unless it names TF2.
fn card_folder(card: &str) -> Result<&'static Path, Refused> {
    match card {
        "tf2" if cards::tf2_mounted() => Ok(Path::new(TF2_ROM_DIR)),
        "tf2" => Err(NO_CARD),
        _ => Ok(Path::new(ROM_DIR)),
    }
}

/// A game, into its console's folder on the card `root` is on. A ZIP goes in a hidden file
/// in `roms` first, until the game inside tells its console.
fn upload_game(request: &mut Request, name: &str, root: &Path, shared: &Shared) -> Result<ResponseBox, Refused> {
    if !valid_file_name(name) {
        return Err(BAD_NAME);
    }
    let size = request.body_length().ok_or(NO_LENGTH)? as u64;
    if size > LARGEST_FILE {
        return Err(TOO_LARGE);
    }
    if free_space(root).is_some_and(|free| free < size + KEPT_FREE) {
        return Err(NO_ROOM);
    }
    let roms = find_roms(root);
    let extension = extension_of(name);
    let console = if extension == "zip" { None } else { Some(console_for_upload(&extension).ok_or(NOT_A_GAME)?) };
    let staging = console.map_or_else(|| root.to_path_buf(), |system| folder_for(system, &roms, root));
    std::fs::create_dir_all(&staging).map_err(|_| NOT_WRITTEN)?;
    let temporary = staging.join(upload_name(name));
    receive(request.as_reader(), &temporary, size, name, shared)?;

    let system = match console {
        Some(system) => system,
        None => match rom_zip::game_in(&temporary).and_then(|game| system_for_extension(&game.extension)) {
            Some(system) => system,
            None => {
                let _ = std::fs::remove_file(&temporary);
                shared.finished(Done::Failed(name.to_string()), false);
                return Err(NOT_A_GAME);
            }
        },
    };
    let folder = folder_for(system, &roms, root);
    let destination = folder.join(name);
    let replaced = destination.exists();
    // A request left running after the server stopped never puts its file in place.
    if shared.stop.load(Ordering::Relaxed) {
        let _ = std::fs::remove_file(&temporary);
        shared.finished(Done::Failed(name.to_string()), false);
        return Err(INCOMPLETE);
    }
    let placed = std::fs::create_dir_all(&folder)
        .and_then(|()| std::fs::rename(&temporary, &destination))
        .and_then(|()| sync_folder(&folder));
    if let Err(error) = placed {
        eprintln!("transfer: could not move {} into place: {error}", destination.display());
        let _ = std::fs::remove_file(&temporary);
        shared.finished(Done::Failed(name.to_string()), false);
        return Err(NOT_WRITTEN);
    }
    println!("transfer: received {} ({size} bytes{})", destination.display(), if replaced { ", replaced" } else { "" });
    shared.finished(Done::Received(name.to_string()), true);
    Ok(json(&Uploaded { path: game_id(&destination), replaced }))
}

/// Reads a whole body of at most `largest` bytes.
fn read_body(request: &mut Request, largest: u64) -> Result<Vec<u8>, Refused> {
    let size = request.body_length().ok_or(NO_LENGTH)? as u64;
    if size > largest {
        return Err(TOO_LARGE);
    }
    let mut body = Vec::new();
    request.as_reader().take(size).read_to_end(&mut body).map_err(|_| INCOMPLETE)?;
    if body.len() as u64 != size {
        return Err(INCOMPLETE);
    }
    Ok(body)
}

/// A game's save, in place of the one it has. Its resume point would load the old save's
/// contents over it, so it is removed.
fn upload_save(request: &mut Request, rom: &Path, shared: &Shared) -> Result<ResponseBox, Refused> {
    let system = system_for(rom).ok_or(NO_GAME)?;
    let body = read_body(request, LARGEST_SAVE)?;
    let save = save_path(rom, system);
    let written = std::fs::create_dir_all(SAVE_DIR).and_then(|()| write_atomic(&save, &body));
    if let Err(error) = written {
        eprintln!("transfer: could not write {}: {error}", save.display());
        return Err(NOT_WRITTEN);
    }
    if let Some(resume) = resume_path(&save, system.persistence) {
        match std::fs::remove_file(&resume) {
            Ok(()) => println!("transfer: removed {}, which would undo the new save", resume.display()),
            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                eprintln!("warning: could not remove {}: {error}", resume.display());
            }
            Err(_) => {}
        }
    }
    println!("transfer: saved {} ({} bytes)", save.display(), body.len());
    shared.finished(Done::SaveReplaced(rom_label(rom)), true);
    Ok(json(&serde_json::json!({ "path": relative(&save, Path::new(crate::device::paths::DATA_DIR)) })))
}

/// A cover, as the page makes it: a PPM image at most the size the launcher shows.
fn upload_cover(request: &mut Request, rom: &Path, shared: &Shared) -> Result<ResponseBox, Refused> {
    let largest = (MAX_SIZE.0 * MAX_SIZE.1 * 3 + 64) as u64;
    let body = read_body(request, largest)?;
    let image = parse_ppm(&body).ok_or(NOT_A_COVER)?;
    if image.width == 0 || image.height == 0 || image.width > MAX_SIZE.0 || image.height > MAX_SIZE.1 {
        return Err(NOT_A_COVER);
    }
    let cover = art_path(Path::new(ART_DIR), rom);
    let written = std::fs::create_dir_all(ART_DIR).and_then(|()| write_atomic(&cover, &body));
    if let Err(error) = written {
        eprintln!("transfer: could not write {}: {error}", cover.display());
        return Err(NOT_WRITTEN);
    }
    println!("transfer: saved the cover {} ({}x{})", cover.display(), image.width, image.height);
    shared.finished(Done::CoverSaved(rom_label(rom)), true);
    Ok(json(&serde_json::json!({})))
}

fn remove_game(rom: &Path, shared: &Shared) -> Result<ResponseBox, Refused> {
    let roms = find_all_roms();
    let library = build_library_with(&roms, &Lists::read(), settings::get().game_names);
    if let Err(error) = delete_game(rom, &library) {
        eprintln!("transfer: could not delete {}: {error}", rom.display());
        return Err(NOT_WRITTEN);
    }
    println!("transfer: deleted {}", rom.display());
    shared.finished(Done::Deleted(rom_label(rom)), true);
    Ok(json(&serde_json::json!({})))
}

/// What starts the name of a game on TF2.
const TF2_PREFIX: &str = "TF2:";

/// A game's name for the page: its path in `roms`, after `TF2:` for one on TF2.
fn game_id(rom: &Path) -> String {
    let root = cards::rom_root(rom);
    let path = relative(rom, root);
    if cards::on_tf2(rom) { format!("{TF2_PREFIX}{path}") } else { path }
}

/// A game on a card, from its name for the page. Only games the launcher lists are found,
/// so a name can't reach anything else.
fn find_game(id: &str) -> Result<PathBuf, Refused> {
    let (root, relative_path) = match id.strip_prefix(TF2_PREFIX) {
        Some(path) => (card_folder("tf2").map_err(|_| NO_GAME)?, path),
        None => (Path::new(ROM_DIR), id),
    };
    let wanted = root.join(relative_path);
    find_roms(root).into_iter().find(|rom| *rom == wanted).ok_or(NO_GAME)
}

fn relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().into_owned()
}

/// The launcher's logo as an SVG image, a square for each of its pixels, so it stays sharp
/// at any size on the page and in the browser's tab.
fn logo_svg() -> String {
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" shape-rendering="crispEdges">"#,
        icons::WIDTH,
        icons::HEIGHT
    );
    for (y, line) in icons::LOGO.iter().enumerate() {
        for (x, c) in line.chars().enumerate() {
            if let Some(Rgb(r, g, b)) = icons::color(c) {
                svg.push_str(&format!(r##"<rect x="{x}" y="{y}" width="1" height="1" fill="#{r:02x}{g:02x}{b:02x}"/>"##));
            }
        }
    }
    svg.push_str("</svg>");
    svg
}

/// A cover as a BMP image, which every browser shows and takes a few lines to write.
fn cover_image(rom: &Path) -> Result<ResponseBox, Refused> {
    let bytes = std::fs::read(art_path(Path::new(ART_DIR), rom)).map_err(|_| NOT_FOUND)?;
    let image = parse_ppm(&bytes).ok_or(NOT_FOUND)?;
    Ok(Response::from_data(bmp(&image))
        .with_header(header("Content-Type", "image/bmp"))
        .with_header(header("Cache-Control", "no-store"))
        .boxed())
}

fn bmp(image: &crate::ui::image::Image) -> Vec<u8> {
    let row = (image.width * 3).div_ceil(4) * 4;
    let data = row * image.height;
    let mut bytes = Vec::with_capacity(54 + data);
    let u32_le = |value: usize| (value as u32).to_le_bytes();
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&u32_le(54 + data));
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&u32_le(54));
    bytes.extend_from_slice(&u32_le(40));
    bytes.extend_from_slice(&u32_le(image.width));
    bytes.extend_from_slice(&u32_le(image.height));
    bytes.extend_from_slice(&[1, 0, 24, 0]);
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&u32_le(data));
    bytes.extend_from_slice(&[0; 16]);
    // Rows go from the bottom up, each pixel blue first.
    for y in (0..image.height).rev() {
        for pixel in &image.pixels[y * image.width..(y + 1) * image.width] {
            bytes.extend_from_slice(&[pixel.2, pixel.1, pixel.0]);
        }
        bytes.resize(bytes.len() + row - image.width * 3, 0);
    }
    bytes
}

/// The folders a browser may download from, under TETSU-DATA.
const DOWNLOADS: [(&str, &str); 2] = [("saves", SAVE_DIR), ("screenshots", SCREENSHOT_DIR)];

/// A file of a download folder, from a path such as `saves/Game.srm`: one name, not hidden.
fn download_path(path: &str) -> Option<PathBuf> {
    let (folder, name) = path.split_once('/')?;
    let (_, directory) = DOWNLOADS.iter().find(|(known, _)| *known == folder)?;
    valid_file_name(name).then(|| Path::new(directory).join(name))
}

fn download(path: &str) -> Result<ResponseBox, Refused> {
    let file = download_path(path).ok_or(NOT_FOUND)?;
    let opened = File::open(&file).map_err(|_| NOT_FOUND)?;
    if !opened.metadata().is_ok_and(|data| data.is_file()) {
        return Err(NOT_FOUND);
    }
    let name = file.file_name().unwrap_or_default().to_string_lossy();
    let content_type = if extension_of(&name) == "png" { "image/png" } else { "application/octet-stream" };
    let encoded: String = name
        .bytes()
        .map(|byte| if byte.is_ascii_alphanumeric() || b"-._".contains(&byte) { char::from(byte).to_string() } else { format!("%{byte:02X}") })
        .collect();
    Ok(Response::from_file(opened)
        .with_header(header("Content-Type", content_type))
        .with_header(header("Content-Disposition", &format!("attachment; filename*=UTF-8''{encoded}")))
        .boxed())
}

/// All of the backup as one ZIP, written as it is sent.
fn download_backup() -> Result<ResponseBox, Refused> {
    let (reader, writer) = io::pipe().map_err(|_| NOT_WRITTEN)?;
    thread::Builder::new()
        .name("transfer-backup".into())
        .spawn(move || match crate::backup::write_backup_zip(crate::backup::data_root(), writer) {
            Ok(files) => println!("transfer: sent a backup of {files} files"),
            // Also when the browser stops the download.
            Err(error) => eprintln!("transfer: the backup ZIP stopped: {error}"),
        })
        .map_err(|_| NOT_WRITTEN)?;
    let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap_or_default();
    let today = crate::device::clock::local(jiff::Timestamp::now(), settings::get().time_zone()).date();
    let name = crate::backup::backup_zip_name(hostname.trim(), today);
    Ok(Response::new(
        200.into(),
        vec![
            header("Content-Type", "application/zip"),
            header("Content-Disposition", &format!("attachment; filename=\"{name}\"")),
            header("Cache-Control", "no-store"),
        ],
        reader,
        None,
        None,
    )
    .boxed())
}

/// A backup ZIP from the page: received whole into a hidden file first, then its files put
/// in place (`backup::restore_zip`).
fn restore_backup(request: &mut Request, shared: &Shared) -> Result<ResponseBox, Refused> {
    let size = request.body_length().ok_or(NO_LENGTH)? as u64;
    let root = Path::new(crate::device::paths::DATA_DIR);
    if size > LARGEST_FILE {
        return Err(TOO_LARGE);
    }
    // Room for the ZIP and for what comes out of it, which is about as large again.
    if free_space(root).is_some_and(|free| free < 2 * size + KEPT_FREE) {
        return Err(NO_ROOM);
    }
    let upload = root.join(upload_name("backup.zip"));
    receive(request.as_reader(), &upload, size, "backup.zip", shared)?;
    let restored = File::open(&upload).and_then(|file| crate::backup::restore_zip(io::BufReader::new(file), root));
    let _ = std::fs::remove_file(&upload);
    match restored {
        Ok(restored) if restored.files > 0 => {
            println!("transfer: restored {} files from a backup, left out {}", restored.files, restored.skipped);
            shared.update(|activity| activity.restored = true);
            shared.finished(Done::Restored(restored.files), true);
            Ok(json(&serde_json::json!({ "files": restored.files, "skipped": restored.skipped })))
        }
        Ok(_) => Err(NOT_A_BACKUP),
        Err(error) if error.kind() == io::ErrorKind::InvalidData => Err(NOT_A_BACKUP),
        Err(error) => {
            eprintln!("transfer: could not restore the backup: {error}");
            Err(NOT_WRITTEN)
        }
    }
}

#[derive(Serialize)]
struct FileListing {
    path: String,
    name: String,
    size: u64,
}

impl FileListing {
    fn of(file: &Path) -> Self {
        Self {
            path: relative(file, Path::new(crate::device::paths::DATA_DIR)),
            name: file.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            size: std::fs::metadata(file).map_or(0, |data| data.len()),
        }
    }
}

#[derive(Serialize)]
struct GameListing {
    path: String,
    name: String,
    size: u64,
    cover: bool,
    saves: Vec<FileListing>,
}

#[derive(Serialize)]
struct ConsoleListing {
    key: &'static str,
    name: &'static str,
    extensions: &'static [&'static str],
    games: Vec<GameListing>,
}

#[derive(Serialize)]
struct CardListing {
    key: &'static str,
    free: Option<u64>,
}

#[derive(Serialize)]
struct Listing {
    language: &'static str,
    /// TETSU-DATA's free space.
    free: Option<u64>,
    /// The cards uploads can go to, TETSU-DATA first, with `tf2` only while one is in.
    cards: Vec<CardListing>,
    /// Every extension the page may send, without the dot.
    accepts: Vec<&'static str>,
    consoles: Vec<ConsoleListing>,
    screenshots: Vec<FileListing>,
}

fn listing() -> Listing {
    let mut consoles: Vec<ConsoleListing> = SYSTEMS
        .iter()
        // Cave Story's program alone is no game, so it can't be sent.
        .map(|system| {
            let extensions = if system.only_titled { &[] } else { system.extensions };
            ConsoleListing { key: system.key, name: system.console, extensions, games: Vec::new() }
        })
        .collect();
    for rom in find_all_roms() {
        let Some(index) = system_index(&rom) else { continue };
        let files = GameFiles::of(&rom);
        consoles[index].games.push(GameListing {
            path: game_id(&rom),
            name: game_title(&rom),
            size: files.size(),
            cover: !files.cover.is_empty(),
            saves: files.player_saves().into_iter().map(|save| FileListing::of(&save)).collect(),
        });
    }
    let mut screenshots: Vec<FileListing> = std::fs::read_dir(SCREENSHOT_DIR)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.file_name().is_some_and(|name| valid_file_name(&name.to_string_lossy())))
        .map(|path| FileListing::of(&path))
        .collect();
    screenshots.sort_by(|a, b| a.name.cmp(&b.name));
    let mut accepts: Vec<&'static str> =
        SYSTEMS.iter().filter(|system| !system.only_titled).flat_map(|system| system.extensions.iter().copied()).collect();
    accepts.extend(DISC_TRACKS);
    accepts.push("zip");
    let free = free_space(Path::new(ROM_DIR));
    let mut cards = vec![CardListing { key: "tf1", free }];
    if cards::tf2_mounted() {
        cards.push(CardListing { key: "tf2", free: free_space(Path::new(TF2_ROM_DIR)) });
    }
    Listing { language: settings::get().language.key(), free, cards, accepts, consoles, screenshots }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::image::Image;

    #[test]
    fn addresses_are_split_and_decoded() {
        let (path, query) = split_url("/api/game?name=Mega%20Man%20%28USA%29.nes&x");
        assert_eq!(path, "/api/game");
        assert_eq!(query["name"], "Mega Man (USA).nes");
        assert_eq!(query["x"], "");
        assert_eq!(split_url("/?code=482+913").1["code"], "482 913");
        assert_eq!(decode("caf%C3%A9").as_deref(), Some("café"));
        assert_eq!(decode("%2"), None, "a cut off escape is refused");
        assert_eq!(decode("%FF"), None, "and so is text that isn't UTF-8");
        assert_eq!(split_url("/?name=%2").1.get("name"), None);
    }

    #[test]
    fn each_upload_has_a_hidden_name_of_its_own() {
        let first = upload_name("Game (USA).zip");
        let second = upload_name("Game (USA).zip");
        assert_ne!(first, second);
        for hidden in [&first, &second] {
            assert!(hidden.starts_with(UPLOAD_PREFIX));
            assert!(hidden.ends_with("-Game (USA).zip"));
            assert_eq!(Path::new(hidden).extension().and_then(|e| e.to_str()), Some("zip"));
        }
    }

    #[test]
    fn the_session_is_found_among_the_cookies() {
        assert_eq!(cookie_token("a=1; tetsu=abc; b=2"), Some("abc"));
        assert_eq!(cookie_token("tetsu2=abc"), None);
        assert_eq!(cookie_token(""), None);
    }

    #[test]
    fn only_plain_file_names_are_taken() {
        assert!(valid_file_name("Pokemon - Emerald Version (USA, Europe).gba"));
        assert!(valid_file_name("Café.nes"));
        for name in ["", ".hidden.gba", "../x.gba", "a/b.gba", "a\\b.gba", "a:b.gba", " x.gba", "x.gba ", "a\nb"] {
            assert!(!valid_file_name(name), "{name:?}");
        }
        assert!(!valid_file_name(&"x".repeat(256)));
    }

    #[test]
    fn uploads_go_to_the_console_of_their_extension() {
        assert_eq!(console_for_upload("gba").map(|system| system.key), Some("gba"));
        assert_eq!(console_for_upload("cue").map(|system| system.key), Some("ps1"));
        assert_eq!(console_for_upload("bin").map(|system| system.key), Some("ps1"), "a disc's tracks go with it");
        assert!(console_for_upload("txt").is_none());
        assert!(console_for_upload("zip").is_none(), "a ZIP waits for what is inside");
    }

    #[test]
    fn new_games_join_the_folder_with_most_of_their_console() {
        let root = Path::new("/roms");
        let gba = system_for_extension("gba").unwrap();
        let ps1 = system_for_extension("cue").unwrap();
        let snes = system_for_extension("sfc").unwrap();
        let roms: Vec<PathBuf> = [
            "/roms/GBA/a.gba",
            "/roms/Old/b.gba",
            "/roms/GBA/c.gba",
            "/roms/PS1/Game A/a.cue",
            "/roms/PS1/Game B/b.cue",
            "/roms/.discs/x.m3u",
            "/roms/top.sfc",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();
        assert_eq!(folder_for(gba, &roms, root), Path::new("/roms/GBA"));
        assert_eq!(folder_for(ps1, &roms, root), Path::new("/roms/PS1"), "games in folders of their own count for the one above");
        assert_eq!(folder_for(snes, &roms, root), Path::new("/roms/SNES"), "a game at the top makes no folder its own");
        let tied: Vec<PathBuf> = ["/roms/Z/a.gba", "/roms/A/b.gba"].iter().map(PathBuf::from).collect();
        assert_eq!(folder_for(gba, &tied, root), Path::new("/roms/A"));
    }

    #[test]
    fn downloads_stay_in_their_folders() {
        assert_eq!(download_path("saves/Game.srm"), Some(Path::new(SAVE_DIR).join("Game.srm")));
        assert_eq!(download_path("screenshots/Game-1.png"), Some(Path::new(SCREENSHOT_DIR).join("Game-1.png")));
        for path in ["saves/../settings.txt", "saves/a/b", "ssh/root-password", "saves/.hidden", "saves", "/saves/x"] {
            assert_eq!(download_path(path), None, "{path}");
        }
    }

    #[test]
    fn covers_are_sent_as_bmp() {
        let image = Image { width: 2, height: 2, pixels: vec![Rgb(1, 2, 3), Rgb(4, 5, 6), Rgb(7, 8, 9), Rgb(10, 11, 12)] };
        let bytes = bmp(&image);
        // Two rows of 6 bytes, each padded to 8.
        assert_eq!(bytes.len(), 54 + 16);
        assert_eq!(&bytes[..2], b"BM");
        assert_eq!(u32::from_le_bytes(bytes[2..6].try_into().unwrap()), 70);
        assert_eq!(&bytes[54..62], &[9, 8, 7, 12, 11, 10, 0, 0], "the bottom row first, blue first");
        assert_eq!(&bytes[62..70], &[3, 2, 1, 6, 5, 4, 0, 0]);
    }

    #[test]
    fn the_logo_is_a_square_for_each_pixel() {
        let svg = logo_svg();
        let pixels: usize = icons::LOGO.iter().map(|line| line.chars().filter(|&c| icons::color(c).is_some()).count()).sum();
        assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
        assert_eq!(svg.matches("<rect").count(), pixels);
    }

    #[test]
    fn progress_is_a_percentage() {
        let receiving = |received, size| Receiving { name: String::new(), received, size };
        assert_eq!(receiving(50, 200).percent(), 25);
        assert_eq!(receiving(0, 0).percent(), 100);
    }
}
