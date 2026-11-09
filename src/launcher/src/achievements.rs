//! `RetroAchievements`, through rcheevos: `rcheevos/` in the source, compiled by `build.rs`.
//!
//! Settings > ACHIEVEMENTS logs in once with a name and password, and keeps the name and
//! the token the server gives in `ACCOUNT_PATH`, never the password. In a game, `Session`
//! logs in with the token, identifies the game by its file's hash, and checks its
//! achievements against the core's memory after every frame. The memory is found through
//! rcheevos' `rc_libretro`, from the memory map a core gives with
//! `RETRO_ENVIRONMENT_SET_MEMORY_MAPS`, or else `retro_get_memory_data`.
//!
//! Requests to the server run on threads with `ureq`, and their answers go back to
//! rcheevos on the thread that made the client, in `Client::pump`, so rcheevos and its
//! callbacks only ever run on that thread. The callbacks' state lives in `STATE`, one per
//! thread, which is never held while calling into rcheevos.

use crate::image::Image;
use crate::color::Rgb;
use crate::i18n::{tr, trf};
use crate::storage::write_atomic;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

/// The account's name and token. Readable only by root, as the token logs in.
pub const ACCOUNT_PATH: &str = "/mnt/data/retroachievements.txt";
/// Achievement badges, downloaded once and kept by their names.
pub const BADGE_DIR: &str = "/mnt/data/achievements";
/// How long a login from Settings may take.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(40);
/// How long leaving a game waits for unlocks still being sent.
const FINISH_TIMEOUT: Duration = Duration::from_secs(5);
/// The size badges are drawn at in the list.
pub const SMALL_BADGE: usize = 32;
/// The size badges are drawn at in a notice, as they are downloaded.
pub const NOTICE_BADGE: usize = 64;

/// The `RetroAchievements` console of a system, by its key in `systems::SYSTEMS`. The
/// Tamagotchi has none.
pub fn console_id(key: &str) -> Option<u32> {
    Some(match key {
        "md" => 1,
        "snes" => 3,
        "gb" => 4,
        "gba" => 5,
        "gbc" => 6,
        "nes" => 7,
        "pce" => 8,
        "sms" => 11,
        "ps1" => 12,
        "lynx" => 13,
        "ngp" | "ngpc" => 14,
        "gg" => 15,
        "ws" | "wsc" => 53,
        _ => return None,
    })
}

#[allow(non_camel_case_types)]
mod ffi {
    use std::ffi::{c_char, c_int, c_void};

    pub enum rc_client_t {}

    /// Only the fields read here; rcheevos owns the rest.
    #[repr(C)]
    pub struct rc_api_request_t {
        pub url: *const c_char,
        pub post_data: *const c_char,
        pub content_type: *const c_char,
    }

    #[repr(C)]
    pub struct rc_api_server_response_t {
        pub body: *const c_char,
        pub body_length: usize,
        pub http_status_code: c_int,
    }

    /// The fields before the ones added in later versions, which aren't read.
    #[repr(C)]
    pub struct rc_client_user_t {
        pub display_name: *const c_char,
        pub username: *const c_char,
        pub token: *const c_char,
        pub score: u32,
        pub score_softcore: u32,
    }

    #[repr(C)]
    pub struct rc_client_game_t {
        pub id: u32,
        pub console_id: u32,
        pub title: *const c_char,
        pub hash: *const c_char,
        pub badge_name: *const c_char,
        pub badge_url: *const c_char,
    }

    #[repr(C)]
    #[derive(Default)]
    pub struct rc_client_user_game_summary_t {
        pub num_core_achievements: u32,
        pub num_unofficial_achievements: u32,
        pub num_unlocked_achievements: u32,
        pub num_unsupported_achievements: u32,
        pub points_core: u32,
        pub points_unlocked: u32,
        pub beaten_time: i64,
        pub completed_time: i64,
    }

    #[repr(C)]
    pub struct rc_client_achievement_t {
        pub title: *const c_char,
        pub description: *const c_char,
        pub badge_name: [c_char; 8],
        pub measured_progress: [c_char; 24],
        pub measured_percent: f32,
        pub id: u32,
        pub points: u32,
        pub unlock_time: i64,
        pub state: u8,
        pub category: u8,
        pub bucket: u8,
        pub unlocked: u8,
        pub rarity: f32,
        pub rarity_hardcore: f32,
        pub type_: u8,
        pub badge_url: *const c_char,
        pub badge_locked_url: *const c_char,
    }

    #[repr(C)]
    pub struct rc_client_achievement_bucket_t {
        pub achievements: *const *const rc_client_achievement_t,
        pub num_achievements: u32,
        pub label: *const c_char,
        pub subset_id: u32,
        pub bucket_type: u8,
    }

    #[repr(C)]
    pub struct rc_client_achievement_list_t {
        pub buckets: *const rc_client_achievement_bucket_t,
        pub num_buckets: u32,
    }

    #[repr(C)]
    pub struct rc_client_leaderboard_t {
        pub title: *const c_char,
        pub description: *const c_char,
        pub tracker_value: *const c_char,
        pub id: u32,
        pub state: u8,
        pub format: u8,
        pub lower_is_better: u8,
    }

    #[repr(C)]
    pub struct rc_client_server_error_t {
        pub error_message: *const c_char,
        pub api: *const c_char,
        pub result: c_int,
        pub related_id: u32,
    }

    #[repr(C)]
    pub struct rc_client_event_t {
        pub type_: u32,
        pub achievement: *const rc_client_achievement_t,
        pub leaderboard: *const rc_client_leaderboard_t,
        pub leaderboard_tracker: *const c_void,
        pub leaderboard_scoreboard: *const c_void,
        pub server_error: *const rc_client_server_error_t,
        pub subset: *const c_void,
    }

    pub const MAX_MEMORY_REGIONS: usize = 32;

    #[repr(C)]
    pub struct rc_libretro_memory_regions_t {
        pub data: [*mut u8; MAX_MEMORY_REGIONS],
        pub size: [usize; MAX_MEMORY_REGIONS],
        pub total_size: usize,
        pub count: u32,
    }

    #[repr(C)]
    pub struct rc_libretro_core_memory_info_t {
        pub data: *mut u8,
        pub size: usize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct retro_memory_descriptor {
        pub flags: u64,
        pub ptr: *mut c_void,
        pub offset: usize,
        pub start: usize,
        pub select: usize,
        pub disconnect: usize,
        pub len: usize,
        pub addrspace: *const c_char,
    }

    #[repr(C)]
    pub struct retro_memory_map {
        pub descriptors: *const retro_memory_descriptor,
        pub num_descriptors: u32,
    }

    pub enum rc_disallowed_setting_t {}

    pub type ServerCallback = unsafe extern "C" fn(*const rc_api_server_response_t, *mut c_void);
    pub type ServerCall = unsafe extern "C" fn(*const rc_api_request_t, ServerCallback, *mut c_void, *mut rc_client_t);
    pub type ReadMemory = unsafe extern "C" fn(u32, *mut u8, u32, *mut rc_client_t) -> u32;
    pub type Callback = unsafe extern "C" fn(c_int, *const c_char, *mut rc_client_t, *mut c_void);
    pub type EventHandler = unsafe extern "C" fn(*const rc_client_event_t, *mut rc_client_t);
    pub type Message = unsafe extern "C" fn(*const c_char, *const rc_client_t);
    pub type CoreMemoryInfo = unsafe extern "C" fn(u32, *mut rc_libretro_core_memory_info_t);

    pub const LOG_LEVEL_INFO: c_int = 3;
    pub const CATEGORY_CORE: c_int = 1;
    pub const GROUPING_LOCK_STATE: c_int = 0;
    pub const RETRYABLE_CLIENT_ERROR: c_int = -2;

    pub const EVENT_ACHIEVEMENT_TRIGGERED: u32 = 1;
    pub const EVENT_LEADERBOARD_STARTED: u32 = 2;
    pub const EVENT_LEADERBOARD_FAILED: u32 = 3;
    pub const EVENT_LEADERBOARD_SUBMITTED: u32 = 4;
    pub const EVENT_PROGRESS_INDICATOR_SHOW: u32 = 7;
    pub const EVENT_PROGRESS_INDICATOR_UPDATE: u32 = 9;
    pub const EVENT_GAME_COMPLETED: u32 = 15;
    pub const EVENT_SERVER_ERROR: u32 = 16;
    pub const EVENT_DISCONNECTED: u32 = 17;
    pub const EVENT_RECONNECTED: u32 = 18;

    pub const RC_NO_GAME_LOADED: c_int = -29;
    pub const RC_NO_RESPONSE: c_int = -32;
    pub const RC_INVALID_CREDENTIALS: c_int = -34;
    pub const RC_EXPIRED_TOKEN: c_int = -35;

    pub const BUCKET_LOCKED: u8 = 1;
    pub const BUCKET_UNLOCKED: u8 = 2;
    pub const BUCKET_UNSUPPORTED: u8 = 3;
    pub const BUCKET_RECENTLY_UNLOCKED: u8 = 5;
    pub const BUCKET_ACTIVE_CHALLENGE: u8 = 6;
    pub const BUCKET_ALMOST_THERE: u8 = 7;

    extern "C" {
        pub fn rc_client_create(read_memory: ReadMemory, server_call: ServerCall) -> *mut rc_client_t;
        pub fn rc_client_destroy(client: *mut rc_client_t);
        pub fn rc_client_set_hardcore_enabled(client: *mut rc_client_t, enabled: c_int);
        pub fn rc_client_get_hardcore_enabled(client: *const rc_client_t) -> c_int;
        pub fn rc_client_enable_logging(client: *mut rc_client_t, level: c_int, callback: Message);
        pub fn rc_client_set_event_handler(client: *mut rc_client_t, handler: EventHandler);
        pub fn rc_client_get_user_agent_clause(client: *mut rc_client_t, buffer: *mut c_char, size: usize) -> usize;
        pub fn rc_client_begin_login_with_password(
            client: *mut rc_client_t,
            username: *const c_char,
            password: *const c_char,
            callback: Callback,
            userdata: *mut c_void,
        ) -> *mut c_void;
        pub fn rc_client_begin_login_with_token(
            client: *mut rc_client_t,
            username: *const c_char,
            token: *const c_char,
            callback: Callback,
            userdata: *mut c_void,
        ) -> *mut c_void;
        pub fn rc_client_get_user_info(client: *const rc_client_t) -> *const rc_client_user_t;
        pub fn rc_client_begin_identify_and_load_game(
            client: *mut rc_client_t,
            console_id: u32,
            file_path: *const c_char,
            data: *const u8,
            data_size: usize,
            callback: Callback,
            userdata: *mut c_void,
        ) -> *mut c_void;
        pub fn rc_client_begin_identify_and_change_media(
            client: *mut rc_client_t,
            file_path: *const c_char,
            data: *const u8,
            data_size: usize,
            callback: Callback,
            userdata: *mut c_void,
        ) -> *mut c_void;
        pub fn rc_client_get_game_info(client: *const rc_client_t) -> *const rc_client_game_t;
        pub fn rc_client_has_achievements(client: *mut rc_client_t) -> c_int;
        pub fn rc_client_get_user_game_summary(client: *const rc_client_t, summary: *mut rc_client_user_game_summary_t);
        pub fn rc_client_create_achievement_list(
            client: *mut rc_client_t,
            category: c_int,
            grouping: c_int,
        ) -> *mut rc_client_achievement_list_t;
        pub fn rc_client_destroy_achievement_list(list: *mut rc_client_achievement_list_t);
        pub fn rc_client_do_frame(client: *mut rc_client_t);
        pub fn rc_client_idle(client: *mut rc_client_t);
        pub fn rc_client_deserialize_progress_sized(client: *mut rc_client_t, data: *const u8, size: usize) -> c_int;

        pub fn rc_libretro_memory_init(
            regions: *mut rc_libretro_memory_regions_t,
            mmap: *const retro_memory_map,
            get_core_memory_info: CoreMemoryInfo,
            console_id: u32,
        ) -> c_int;
        pub fn rc_libretro_init_verbose_message_callback(callback: unsafe extern "C" fn(*const c_char));
        pub fn rc_libretro_memory_read(
            regions: *const rc_libretro_memory_regions_t,
            address: u32,
            buffer: *mut u8,
            num_bytes: u32,
        ) -> u32;
        pub fn rc_libretro_get_disallowed_settings(library_name: *const c_char) -> *const rc_disallowed_setting_t;
        pub fn rc_libretro_is_setting_allowed(
            settings: *const rc_disallowed_setting_t,
            setting: *const c_char,
            value: *const c_char,
        ) -> c_int;
    }
}

/// Text from rcheevos, or empty for none.
fn text(pointer: *const c_char) -> String {
    if pointer.is_null() {
        return String::new();
    }
    // SAFETY: rcheevos' strings end in a zero, and outlive the call that gave them.
    unsafe { CStr::from_ptr(pointer) }.to_string_lossy().into_owned()
}

/// Text from a fixed-size field, which ends at its first zero.
fn field_text(field: &[c_char]) -> String {
    let bytes: Vec<u8> = field.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// What was logged in with, kept in `ACCOUNT_PATH`.
#[derive(Clone, Debug, PartialEq)]
pub struct Account {
    pub user: String,
    pub token: String,
}

impl Account {
    pub fn read(path: &Path) -> Option<Self> {
        Self::parse(&std::fs::read_to_string(path).ok()?)
    }

    fn parse(text: &str) -> Option<Self> {
        let value = |key: &str| {
            text.lines().find_map(|line| line.strip_prefix(key)?.strip_prefix('=')).map(str::trim).filter(|value| !value.is_empty())
        };
        Some(Self { user: value("user")?.to_string(), token: value("token")?.to_string() })
    }

    pub fn write(&self, path: &Path) -> io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        write_atomic(path, format!("user={}\ntoken={}\n", self.user, self.token).as_bytes())?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
    }
}

/// Forgets the account, which logs out.
pub fn log_out(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// A finished request, for `Client::pump` to hand to rcheevos.
struct Response {
    callback: ffi::ServerCallback,
    /// rcheevos' pointer for the callback, carried across threads as a number.
    data: usize,
    status: c_int,
    body: Vec<u8>,
}

/// Something rcheevos reported from inside one of its calls, for the game to show.
#[derive(Clone, Debug, PartialEq)]
enum Happening {
    Unlocked { title: String, points: u32, badge: String },
    Progress { title: String, progress: String, badge: String },
    LeaderboardStarted(String),
    LeaderboardFailed(String),
    LeaderboardSubmitted { title: String, value: String },
    Completed,
    ServerError(String),
    Disconnected,
    Reconnected,
}

/// The result of a call that finished, from its callback: Ok, or the code and message.
type Outcome = Result<(), (c_int, String)>;

/// What the callbacks need, one per thread with a client.
struct State {
    sender: Sender<Response>,
    in_flight: usize,
    happenings: Vec<Happening>,
    login: Option<Outcome>,
    load: Option<Outcome>,
    user_agent: String,
    regions: Option<Box<ffi::rc_libretro_memory_regions_t>>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    /// The core's memory map, kept from when it gave it until the memory is set up.
    static MEMORY_MAP: RefCell<Option<MemoryMap>> = const { RefCell::new(None) };
    /// Set when a core gives a memory map, until the session lays out the memory again.
    static MAP_CHANGED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// `retro_get_memory_data` and `retro_get_memory_size`.
    static CORE_MEMORY: std::cell::Cell<Option<CoreMemory>> = const { std::cell::Cell::new(None) };
}

/// Runs `action` with the state, if a client has one. rcheevos' callbacks use this, so
/// nothing may call into rcheevos from inside `action`.
fn with_state<T>(action: impl FnOnce(&mut State) -> T) -> Option<T> {
    STATE.with(|state| state.borrow_mut().as_mut().map(action))
}

/// The core's own functions for its memory.
#[derive(Clone, Copy)]
pub struct CoreMemory {
    pub data: unsafe extern "C" fn(u32) -> *mut c_void,
    pub size: unsafe extern "C" fn(u32) -> usize,
}

/// A copy of a core's memory map, as the core's own may not outlive the call.
struct MemoryMap {
    descriptors: Vec<ffi::retro_memory_descriptor>,
    _names: Vec<CString>,
}

/// Keeps the memory map a core gives with `RETRO_ENVIRONMENT_SET_MEMORY_MAPS`.
///
/// # Safety
/// `data` must point to a `retro_memory_map` as libretro.h has it.
pub unsafe fn keep_memory_map(data: *const c_void) {
    let mut descriptors = Vec::new();
    let mut names = Vec::new();
    // SAFETY: the caller promises a `retro_memory_map`, whose `descriptors` has
    // `num_descriptors` entries, each with a name that is null or a C string.
    unsafe {
        let map = &*data.cast::<ffi::retro_memory_map>();
        if !map.descriptors.is_null() {
            for index in 0..map.num_descriptors as usize {
                let mut descriptor = *map.descriptors.add(index);
                if !descriptor.addrspace.is_null() {
                    let name = CStr::from_ptr(descriptor.addrspace).to_owned();
                    descriptor.addrspace = name.as_ptr();
                    names.push(name);
                }
                descriptors.push(descriptor);
            }
        }
    }
    println!("achievements: the core gave {} memory descriptors", descriptors.len());
    MEMORY_MAP.with(|slot| *slot.borrow_mut() = Some(MemoryMap { descriptors, _names: names }));
    MAP_CHANGED.with(|changed| changed.set(true));
}

/// Tells rcheevos where one of the core's memory areas is.
///
/// # Safety
/// Called by rcheevos, on the thread that runs the core, with an `info` to fill in.
unsafe extern "C" fn core_memory_info(id: u32, info: *mut ffi::rc_libretro_core_memory_info_t) {
    let (data, size) = match CORE_MEMORY.with(|memory| memory.get()) {
        // SAFETY: the core's own functions, which stay valid while it is loaded. This is
        // the thread that runs it.
        Some(memory) => unsafe { ((memory.data)(id).cast::<u8>(), (memory.size)(id)) },
        None => (std::ptr::null_mut(), 0),
    };
    // SAFETY: rcheevos passes a struct of its own to fill in.
    unsafe {
        (*info).data = data;
        (*info).size = if data.is_null() { 0 } else { size };
    }
}

/// Reads the game's memory for rcheevos.
///
/// # Safety
/// Called by rcheevos, on the thread that runs the core, with room for `count` bytes at
/// `buffer`.
unsafe extern "C" fn read_memory(address: u32, buffer: *mut u8, count: u32, _: *mut ffi::rc_client_t) -> u32 {
    STATE.with(|state| match state.borrow().as_ref().and_then(|state| state.regions.as_deref()) {
        // SAFETY: the regions were laid out from the loaded core's memory, and rcheevos
        // copies at most `count` bytes into its own buffer.
        Some(regions) => unsafe { ffi::rc_libretro_memory_read(regions, address, buffer, count) },
        None => 0,
    })
}

/// Sends a request for rcheevos on a thread of its own. The answer comes back to the
/// thread that runs the core, which hands it to `callback`.
///
/// # Safety
/// Called by rcheevos with a `request` that is valid for the call.
unsafe extern "C" fn server_call(
    request: *const ffi::rc_api_request_t,
    callback: ffi::ServerCallback,
    data: *mut c_void,
    _: *mut ffi::rc_client_t,
) {
    // SAFETY: rcheevos passes a request that is valid for the call.
    let request = unsafe { &*request };
    let url = text(request.url);
    let post = (!request.post_data.is_null()).then(|| text(request.post_data));
    let content_type = text(request.content_type);
    let Some((sender, user_agent)) = with_state(|state| {
        state.in_flight += 1;
        (state.sender.clone(), state.user_agent.clone())
    }) else {
        return;
    };
    let data = data as usize;
    #[cfg(test)]
    if let Some(fake) = tests::FAKE_SERVER.with(|fake| fake.get()) {
        let (status, body) = fake(post.as_deref().unwrap_or(&url));
        let _ = sender.send(Response { callback, data, status, body });
        return;
    }
    std::thread::spawn(move || {
        let (status, body) = send(&url, post.as_deref(), &content_type, &user_agent);
        // Fails only once the client is gone, which no longer wants the answer.
        let _ = sender.send(Response { callback, data, status, body });
    });
}

/// Sends a request, returning the HTTP status and body. A request that never reached the
/// server is retried by rcheevos where that makes sense, as unlocks do.
fn send(url: &str, post: Option<&str>, content_type: &str, user_agent: &str) -> (c_int, Vec<u8>) {
    let agent: ureq::Agent = crate::http::config(Duration::from_secs(30)).http_status_as_error(false).build().into();
    let result = match post {
        Some(body) => {
            let content_type = if content_type.is_empty() { "application/x-www-form-urlencoded" } else { content_type };
            agent.post(url).header("User-Agent", user_agent).header("Content-Type", content_type).send(body.as_bytes())
        }
        None => agent.get(url).header("User-Agent", user_agent).call(),
    };
    match result.and_then(|mut response| {
        let status = response.status().as_u16();
        Ok((c_int::from(status), response.body_mut().read_to_vec()?))
    }) {
        Ok(answer) => answer,
        Err(error) => {
            eprintln!("achievements: request failed: {error}");
            (ffi::RETRYABLE_CLIENT_ERROR, Vec::new())
        }
    }
}

unsafe extern "C" fn log_message(message: *const c_char, _: *const ffi::rc_client_t) {
    println!("achievements: {}", text(message));
}

unsafe extern "C" fn login_done(result: c_int, message: *const c_char, _: *mut ffi::rc_client_t, _: *mut c_void) {
    let outcome = if result == 0 { Ok(()) } else { Err((result, text(message))) };
    with_state(|state| state.login = Some(outcome));
}

unsafe extern "C" fn load_done(result: c_int, message: *const c_char, _: *mut ffi::rc_client_t, _: *mut c_void) {
    let outcome = if result == 0 { Ok(()) } else { Err((result, text(message))) };
    with_state(|state| state.load = Some(outcome));
}

unsafe extern "C" fn media_changed(result: c_int, message: *const c_char, _: *mut ffi::rc_client_t, _: *mut c_void) {
    if result != 0 {
        eprintln!("achievements: the new disc wasn't recognised: {}", text(message));
    }
}

/// Keeps what rcheevos reports, to show it.
///
/// # Safety
/// Called by rcheevos with an `event` that is valid for the call.
unsafe extern "C" fn event_handler(event: *const ffi::rc_client_event_t, _: *mut ffi::rc_client_t) {
    // SAFETY: rcheevos passes an event that is valid for the call. Each of its pointers is
    // null or points to something that lasts as long.
    let (event, achievement, board, server_error) = unsafe {
        let event = &*event;
        (event, event.achievement.as_ref(), event.leaderboard.as_ref(), event.server_error.as_ref())
    };
    let leaderboard = board.map(|leaderboard| text(leaderboard.title)).unwrap_or_default();
    let happening = match event.type_ {
        ffi::EVENT_ACHIEVEMENT_TRIGGERED => achievement.map(|achievement| Happening::Unlocked {
            title: text(achievement.title),
            points: achievement.points,
            badge: field_text(&achievement.badge_name),
        }),
        ffi::EVENT_PROGRESS_INDICATOR_SHOW | ffi::EVENT_PROGRESS_INDICATOR_UPDATE => {
            achievement.map(|achievement| Happening::Progress {
                title: text(achievement.title),
                progress: field_text(&achievement.measured_progress),
                badge: field_text(&achievement.badge_name),
            })
        }
        ffi::EVENT_LEADERBOARD_STARTED => Some(Happening::LeaderboardStarted(leaderboard)),
        ffi::EVENT_LEADERBOARD_FAILED => Some(Happening::LeaderboardFailed(leaderboard)),
        ffi::EVENT_LEADERBOARD_SUBMITTED => Some(Happening::LeaderboardSubmitted {
            value: board.map(|leaderboard| text(leaderboard.tracker_value)).unwrap_or_default(),
            title: leaderboard,
        }),
        ffi::EVENT_GAME_COMPLETED => Some(Happening::Completed),
        ffi::EVENT_SERVER_ERROR => {
            Some(Happening::ServerError(server_error.map(|error| text(error.error_message)).unwrap_or_default()))
        }
        ffi::EVENT_DISCONNECTED => Some(Happening::Disconnected),
        ffi::EVENT_RECONNECTED => Some(Happening::Reconnected),
        _ => None,
    };
    if let Some(happening) = happening {
        println!("achievements: {happening:?}");
        with_state(|state| state.happenings.push(happening));
    }
}

/// An rcheevos client, on the thread that made it.
struct Client {
    raw: *mut ffi::rc_client_t,
    responses: Receiver<Response>,
}

impl Client {
    fn new() -> Result<Self, String> {
        // SAFETY: the callbacks have the signatures rc_client.h gives them.
        let raw = unsafe { ffi::rc_client_create(read_memory, server_call) };
        if raw.is_null() {
            return Err("rcheevos could not start".into());
        }
        let mut clause = [0 as c_char; 64];
        // SAFETY: the buffer has the size given, and the client was just made.
        unsafe {
            ffi::rc_client_get_user_agent_clause(raw, clause.as_mut_ptr(), clause.len());
            ffi::rc_client_enable_logging(raw, ffi::LOG_LEVEL_INFO, log_message);
            ffi::rc_client_set_event_handler(raw, event_handler);
        }
        let (sender, responses) = channel();
        let user_agent = format!("TetsuOS/{} (Linux) {}", env!("CARGO_PKG_VERSION"), field_text(&clause));
        STATE.with(|state| {
            *state.borrow_mut() = Some(State {
                sender,
                in_flight: 0,
                happenings: Vec::new(),
                login: None,
                load: None,
                user_agent,
                regions: None,
            });
        });
        Ok(Self { raw, responses })
    }

    /// Hands rcheevos the answers that have arrived.
    fn pump(&self) {
        for response in self.responses.try_iter() {
            with_state(|state| state.in_flight = state.in_flight.saturating_sub(1));
            let answer = ffi::rc_api_server_response_t {
                body: response.body.as_ptr().cast::<c_char>(),
                body_length: response.body.len(),
                http_status_code: response.status,
            };
            // SAFETY: rcheevos gave this callback and its data with the request, and the
            // client that made the request still exists.
            unsafe { (response.callback)(&raw const answer, response.data as *mut c_void) };
        }
    }

    fn in_flight(&self) -> usize {
        with_state(|state| state.in_flight).unwrap_or(0)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // SAFETY: the client was made by rc_client_create, and nothing uses it after this.
        unsafe { ffi::rc_client_destroy(self.raw) };
        STATE.with(|state| *state.borrow_mut() = None);
    }
}

/// What a failed login or start says.
fn failure_text(code: c_int, message: &str) -> String {
    match code {
        ffi::RC_INVALID_CREDENTIALS => tr("WRONG NAME OR PASSWORD").to_string(),
        ffi::RC_EXPIRED_TOKEN => tr("LOG IN AGAIN IN SETTINGS").to_string(),
        ffi::RC_NO_RESPONSE => tr("NO CONNECTION TO RETROACHIEVEMENTS").to_string(),
        _ if message.is_empty() => tr("NO CONNECTION TO RETROACHIEVEMENTS").to_string(),
        _ => message.to_uppercase(),
    }
}

/// Logs in with a password, and returns what to keep. Waits for the server, so it runs on
/// a thread of its own.
pub fn log_in(user: &str, password: &str) -> Result<Account, String> {
    let client = Client::new()?;
    let (Ok(user), Ok(password)) = (CString::new(user.trim()), CString::new(password)) else {
        return Err(tr("WRONG NAME OR PASSWORD").to_string());
    };
    // SAFETY: the strings outlive the call, which copies them.
    unsafe {
        ffi::rc_client_begin_login_with_password(client.raw, user.as_ptr(), password.as_ptr(), login_done, std::ptr::null_mut());
    }
    let deadline = Instant::now() + LOGIN_TIMEOUT;
    let outcome = loop {
        client.pump();
        if let Some(outcome) = with_state(|state| state.login.take()).flatten() {
            break outcome;
        }
        if Instant::now() > deadline {
            return Err(tr("NO CONNECTION TO RETROACHIEVEMENTS").to_string());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    if let Err((code, message)) = outcome {
        eprintln!("achievements: login failed ({code}): {message}");
        return Err(failure_text(code, &message));
    }
    // SAFETY: logged in, so the client has the user, which lasts as long as it.
    let user = unsafe { ffi::rc_client_get_user_info(client.raw).as_ref() }.ok_or("no user")?;
    let account = Account { user: text(user.username), token: text(user.token) };
    println!("achievements: logged in as {} ({} points)", account.user, user.score);
    Ok(account)
}

/// How far loading the game has got.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Loading,
    Ready,
    /// The server knows the game, but it has no achievements yet.
    NoAchievements,
    NotRecognised,
    Failed(String),
}

/// A message shown over the game for a few seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    /// What happened, in the menus' language.
    pub heading: String,
    /// The achievement, game or leaderboard, as the server names it.
    pub title: String,
    /// The badge to show beside it, by name.
    pub badge: Option<String>,
    pub time: Duration,
    /// Progress towards an achievement, which a newer one replaces while it waits.
    pub progress: bool,
}

impl Notice {
    pub fn new(heading: impl Into<String>, title: impl Into<String>) -> Self {
        Self { heading: heading.into(), title: title.into(), badge: None, time: Duration::from_secs(4), progress: false }
    }
}

/// Queues notices to show one after another. Progress towards an achievement replaces any
/// still waiting, as it changes often and only the latest matters.
pub fn queue(waiting: &mut std::collections::VecDeque<Notice>, new: Vec<Notice>) {
    for notice in new {
        if notice.progress {
            waiting.retain(|other| !other.progress);
        }
        waiting.push_back(notice);
    }
}

/// An achievement on the progress page.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub title: String,
    pub description: String,
    pub points: u32,
    pub unlocked: bool,
    /// Such as "3/10", for one that counts towards its goal.
    pub progress: String,
    pub badge: String,
}

/// A heading on the progress page, or an achievement under it.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Heading(&'static str),
    Achievement(Entry),
}

/// Unlocked against all, as the details screen and the progress page show it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Summary {
    pub unlocked: u32,
    pub total: u32,
    pub points: u32,
    pub points_total: u32,
    pub hardcore: bool,
}

impl Summary {
    fn to_text(self) -> String {
        format!(
            "unlocked={}\ntotal={}\npoints={}\npoints_total={}\nhardcore={}\n",
            self.unlocked, self.total, self.points, self.points_total, self.hardcore
        )
    }

    fn parse(text: &str) -> Option<Self> {
        let value = |key: &str| text.lines().find_map(|line| line.strip_prefix(key)?.strip_prefix('='));
        let number = |key: &str| value(key)?.trim().parse().ok();
        Some(Self {
            unlocked: number("unlocked")?,
            total: number("total")?,
            points: number("points").unwrap_or(0),
            points_total: number("points_total").unwrap_or(0),
            hardcore: value("hardcore").is_some_and(|value| value.trim() == "true"),
        })
    }

    pub fn read(path: &Path) -> Option<Self> {
        Self::parse(&std::fs::read_to_string(path).ok()?)
    }
}

/// The file beside a game's save that says how many of its achievements are unlocked, for
/// the launcher's details screen, which can't ask rcheevos.
pub fn summary_path(save: &Path) -> PathBuf {
    let mut name = save.as_os_str().to_owned();
    name.push(".achievements");
    PathBuf::from(name)
}

/// The file beside a game's save that turns `RetroAchievements` off for it.
pub fn off_path(save: &Path) -> PathBuf {
    let mut name = save.as_os_str().to_owned();
    name.push(".achievements-off");
    PathBuf::from(name)
}

pub fn is_off_for_game(save: &Path) -> bool {
    off_path(save).exists()
}

/// Turns `RetroAchievements` off for the game with this save, or on again.
pub fn set_off_for_game(save: &Path, off: bool) -> io::Result<()> {
    let path = off_path(save);
    if off {
        write_atomic(&path, b"")
    } else {
        match std::fs::remove_file(&path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }
}

pub fn is_off_file(file: &Path) -> bool {
    file.extension().is_some_and(|extension| extension == "achievements-off")
}

/// Settings > ADVANCED > RESET SETTINGS: achievements on again for every game.
pub fn remove_all_off(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for path in entries.filter_map(|entry| Some(entry.ok()?.path())).filter(|path| is_off_file(path)) {
        if let Err(error) = std::fs::remove_file(&path) {
            eprintln!("warning: could not remove {}: {error}", path.display());
        }
    }
}

pub fn is_summary(file: &Path) -> bool {
    file.extension().is_some_and(|extension| extension == "achievements")
}

/// The heading of a group on the progress page.
fn bucket_label(bucket: u8) -> &'static str {
    match bucket {
        ffi::BUCKET_RECENTLY_UNLOCKED => tr("RECENTLY UNLOCKED"),
        ffi::BUCKET_ACTIVE_CHALLENGE => tr("CHALLENGES UNDER WAY"),
        ffi::BUCKET_ALMOST_THERE => tr("ALMOST THERE"),
        ffi::BUCKET_UNLOCKED => tr("UNLOCKED"),
        ffi::BUCKET_UNSUPPORTED => tr("NOT SUPPORTED HERE"),
        ffi::BUCKET_LOCKED => tr("LOCKED"),
        _ => tr("OTHERS"),
    }
}

/// How much of the core's memory rcheevos finds for a console, in bytes and regions, as
/// `--render` reports it to check a core away from the menus.
pub fn memory_found(console: u32, memory: CoreMemory) -> (usize, u32) {
    unsafe extern "C" fn verbose(message: *const c_char) {
        println!("rc_libretro: {}", text(message));
    }
    // SAFETY: the callback has the signature rc_libretro.h gives it.
    unsafe { ffi::rc_libretro_init_verbose_message_callback(verbose) };
    CORE_MEMORY.with(|slot| slot.set(Some(memory)));
    let regions = init_regions(console);
    (regions.total_size, regions.count)
}

/// Lays the console's address space over the core's memory map, or its memory.
fn init_regions(console: u32) -> Box<ffi::rc_libretro_memory_regions_t> {
    let mut regions = Box::new(ffi::rc_libretro_memory_regions_t {
        data: [std::ptr::null_mut(); ffi::MAX_MEMORY_REGIONS],
        size: [0; ffi::MAX_MEMORY_REGIONS],
        total_size: 0,
        count: 0,
    });
    MEMORY_MAP.with(|map| {
        let map = map.borrow();
        let raw = map.as_ref().filter(|map| !map.descriptors.is_empty()).map(|map| ffi::retro_memory_map {
            descriptors: map.descriptors.as_ptr(),
            num_descriptors: map.descriptors.len() as u32,
        });
        let raw = raw.as_ref().map_or(std::ptr::null(), std::ptr::from_ref);
        // SAFETY: the map and its descriptors outlive the call, which copies what it needs.
        unsafe { ffi::rc_libretro_memory_init(&raw mut *regions, raw, core_memory_info, console) };
    });
    regions
}

/// One game's achievements, from start to leaving.
pub struct Session {
    client: Client,
    console: u32,
    status: Status,
    summary_path: PathBuf,
    /// Frames since the memory was last looked for, while the core had none to show.
    memory_retry: u32,
    badges: Badges,
}

impl Session {
    /// Logs in and starts identifying the game in `rom`, whose save is `save`. With
    /// `hardcore`, unlocks count as hardcore, and the game must keep to its rules.
    pub fn start(account: &Account, console: u32, rom: &Path, save: &Path, hardcore: bool, memory: CoreMemory) -> Result<Self, String> {
        let client = Client::new()?;
        CORE_MEMORY.with(|slot| slot.set(Some(memory)));
        MAP_CHANGED.with(|changed| changed.set(false));
        let mut session = Self {
            client,
            console,
            status: Status::Loading,
            summary_path: summary_path(save),
            memory_retry: 0,
            badges: Badges::new(Path::new(BADGE_DIR)),
        };
        session.find_memory();
        let (Ok(user), Ok(token), Ok(path)) =
            (CString::new(account.user.as_str()), CString::new(account.token.as_str()), crate::libretro::c_path(rom))
        else {
            return Err("unusable account or path".into());
        };
        // SAFETY: the strings outlive the calls, which copy them.
        unsafe {
            ffi::rc_client_set_hardcore_enabled(session.client.raw, c_int::from(hardcore));
            ffi::rc_client_begin_login_with_token(session.client.raw, user.as_ptr(), token.as_ptr(), login_done, std::ptr::null_mut());
            // Waits for the login inside rcheevos.
            ffi::rc_client_begin_identify_and_load_game(
                session.client.raw,
                console,
                path.as_ptr(),
                std::ptr::null(),
                0,
                load_done,
                std::ptr::null_mut(),
            );
        }
        Ok(session)
    }

    /// Sets up reading the core's memory: from its memory map, or the memory it shows
    /// through `retro_get_memory_data`.
    fn find_memory(&mut self) {
        let regions = init_regions(self.console);
        println!("achievements: {} bytes of memory in {} regions", regions.total_size, regions.count);
        with_state(|state| state.regions = Some(regions));
    }

    fn memory_found(&self) -> bool {
        with_state(|state| state.regions.as_ref().is_some_and(|regions| regions.total_size > 0)).unwrap_or(false)
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn hardcore(&self) -> bool {
        // SAFETY: the client exists while the session does.
        unsafe { ffi::rc_client_get_hardcore_enabled(self.client.raw) != 0 }
    }

    /// Drops to softcore for the rest of the game, as a setting broke hardcore's rules.
    pub fn end_hardcore(&mut self) {
        // SAFETY: the client exists while the session does.
        unsafe { ffi::rc_client_set_hardcore_enabled(self.client.raw, 0) };
        self.write_summary();
    }

    /// Whether the game must keep to hardcore's rules: no loading states, cheats or fast
    /// forward. Not once the game has turned out to have no achievements to earn.
    pub fn restricts(&self) -> bool {
        self.hardcore() && matches!(self.status, Status::Loading | Status::Ready)
    }

    /// Hands rcheevos the server's answers, and returns what to show for them.
    pub fn pump(&mut self) -> Vec<Notice> {
        self.client.pump();
        let mut notices = Vec::new();
        if let Some(outcome) = with_state(|state| state.load.take()).flatten() {
            notices.push(self.loaded(outcome));
        }
        if let Some(Err((code, message))) = with_state(|state| state.login.take()).flatten() {
            eprintln!("achievements: login failed ({code}): {message}");
        }
        let happenings = with_state(|state| std::mem::take(&mut state.happenings)).unwrap_or_default();
        let mut unlocked = false;
        for happening in happenings {
            unlocked |= matches!(happening, Happening::Unlocked { .. } | Happening::Completed);
            notices.push(self.notice_for(happening));
        }
        if unlocked {
            self.write_summary();
        }
        notices
    }

    fn loaded(&mut self, outcome: Outcome) -> Notice {
        let hardcore = self.hardcore();
        match outcome {
            Ok(()) => {
                // SAFETY: the client exists while the session does.
                let has_achievements = unsafe { ffi::rc_client_has_achievements(self.client.raw) != 0 };
                // SAFETY: a game is loaded, so the client has its info, which lasts as long.
                let title = unsafe { ffi::rc_client_get_game_info(self.client.raw).as_ref() }.map(|game| text(game.title));
                let title = title.unwrap_or_default();
                if !has_achievements {
                    self.status = Status::NoAchievements;
                    return Notice::new(tr("NO ACHIEVEMENTS FOR THIS GAME YET"), title);
                }
                self.status = Status::Ready;
                let summary = self.write_summary();
                self.badges.download(self.badge_urls());
                let mut heading = trf("{} OF {} ACHIEVEMENTS UNLOCKED", &[&summary.unlocked, &summary.total]);
                if hardcore {
                    heading = format!("{heading} - {}", tr("HARDCORE"));
                }
                Notice::new(heading, title)
            }
            Err((ffi::RC_NO_GAME_LOADED, message)) => {
                println!("achievements: not recognised: {message}");
                self.status = Status::NotRecognised;
                Notice::new(tr("ACHIEVEMENTS"), tr("RETROACHIEVEMENTS DOESN'T KNOW THIS GAME"))
            }
            Err((code, message)) => {
                eprintln!("achievements: could not load the game ({code}): {message}");
                let text = failure_text(code, &message);
                self.status = Status::Failed(text.clone());
                Notice::new(tr("ACHIEVEMENTS"), text)
            }
        }
    }

    fn notice_for(&self, happening: Happening) -> Notice {
        match happening {
            Happening::Unlocked { title, points, badge } => Notice {
                badge: Some(badge),
                time: Duration::from_secs(5),
                ..Notice::new(trf("ACHIEVEMENT UNLOCKED - {} POINTS", &[&points]), title)
            },
            Happening::Progress { title, progress, badge } => Notice {
                badge: Some(badge),
                time: Duration::from_secs(2),
                progress: true,
                ..Notice::new(progress, title)
            },
            Happening::LeaderboardStarted(title) => Notice::new(tr("LEADERBOARD STARTED"), title),
            Happening::LeaderboardFailed(title) => Notice::new(tr("LEADERBOARD FAILED"), title),
            Happening::LeaderboardSubmitted { title, value } => Notice::new(trf("SUBMITTED {}", &[&value]), title),
            Happening::Completed => {
                let heading = if self.hardcore() { tr("GAME MASTERED") } else { tr("GAME COMPLETED") };
                Notice { time: Duration::from_secs(6), ..Notice::new(heading, self.game_title()) }
            }
            Happening::ServerError(message) => Notice::new(tr("RETROACHIEVEMENTS"), message),
            Happening::Disconnected => Notice::new(tr("RETROACHIEVEMENTS"), tr("UNLOCKS WAIT FOR A CONNECTION")),
            Happening::Reconnected => Notice::new(tr("RETROACHIEVEMENTS"), tr("WAITING UNLOCKS SENT")),
        }
    }

    fn game_title(&self) -> String {
        // SAFETY: the client exists while the session does, and so does a game's info.
        unsafe { ffi::rc_client_get_game_info(self.client.raw).as_ref() }.map(|game| text(game.title)).unwrap_or_default()
    }

    /// Checks the achievements against the frame the core just ran.
    pub fn do_frame(&mut self) {
        // mGBA gives its memory map in its first frame, not when the game loads, and before
        // that only shows its work RAM: without this, high RAM at $FF80 was missing, and
        // every Super Mario Land achievement was disabled for reading it. RetroArch does
        // the same on a new map.
        if MAP_CHANGED.with(|changed| changed.replace(false)) {
            self.find_memory();
        } else if self.status == Status::Loading && !self.memory_found() {
            // Some cores only show their memory once the game runs.
            self.memory_retry += 1;
            if self.memory_retry.is_multiple_of(60) {
                self.find_memory();
            }
        }
        debug_assert!(STATE.with(|state| state.try_borrow_mut().is_ok()));
        // SAFETY: the client exists while the session does.
        unsafe { ffi::rc_client_do_frame(self.client.raw) };
    }

    /// Keeps the session alive while the game is paused.
    pub fn idle(&mut self) {
        // SAFETY: the client exists while the session does.
        unsafe { ffi::rc_client_idle(self.client.raw) };
    }

    /// After loading a state, the achievements start over from it, so the jump in memory
    /// doesn't count as progress.
    pub fn reset_progress(&mut self) {
        // SAFETY: no data with no size resets them, as rc_client.h says.
        unsafe { ffi::rc_client_deserialize_progress_sized(self.client.raw, std::ptr::null(), 0) };
    }

    /// Tells rcheevos about a disc change, so a game whose discs have different hashes keeps
    /// its achievements.
    pub fn change_disc(&mut self, disc: &Path) {
        let Ok(path) = crate::libretro::c_path(disc) else { return };
        // SAFETY: the path outlives the call, which copies it.
        unsafe {
            ffi::rc_client_begin_identify_and_change_media(
                self.client.raw,
                path.as_ptr(),
                std::ptr::null(),
                0,
                media_changed,
                std::ptr::null_mut(),
            );
        }
    }

    pub fn summary(&self) -> Summary {
        let mut raw = ffi::rc_client_user_game_summary_t::default();
        // SAFETY: the client exists while the session does, and the struct matches.
        unsafe { ffi::rc_client_get_user_game_summary(self.client.raw, &raw mut raw) };
        Summary {
            unlocked: raw.num_unlocked_achievements,
            total: raw.num_core_achievements,
            points: raw.points_unlocked,
            points_total: raw.points_core,
            hardcore: self.hardcore(),
        }
    }

    fn write_summary(&self) -> Summary {
        let summary = self.summary();
        if self.status == Status::Ready {
            if let Err(error) = write_atomic(&self.summary_path, summary.to_text().as_bytes()) {
                eprintln!("warning: could not save the achievement count: {error}");
            }
        }
        summary
    }

    /// The game's achievements, grouped as rcheevos groups them.
    pub fn lines(&self) -> Vec<Line> {
        let mut lines = Vec::new();
        self.for_each_achievement(|bucket, achievement| {
            if bucket.is_some() {
                lines.push(Line::Heading(bucket_label(bucket.unwrap_or_default())));
            }
            lines.push(Line::Achievement(Entry {
                title: text(achievement.title),
                description: text(achievement.description),
                points: achievement.points,
                unlocked: achievement.unlocked != 0,
                progress: field_text(&achievement.measured_progress),
                badge: field_text(&achievement.badge_name),
            }));
        });
        lines
    }

    fn badge_urls(&self) -> Vec<(String, String)> {
        let mut urls = Vec::new();
        self.for_each_achievement(|_, achievement| {
            urls.push((field_text(&achievement.badge_name), text(achievement.badge_url)));
        });
        urls
    }

    /// Calls `action` with each achievement in the list rcheevos makes, and with the
    /// bucket's type for the first of each bucket.
    fn for_each_achievement(&self, mut action: impl FnMut(Option<u8>, &ffi::rc_client_achievement_t)) {
        // SAFETY: the list and everything in it stay valid until it is destroyed below.
        unsafe {
            let list = ffi::rc_client_create_achievement_list(self.client.raw, ffi::CATEGORY_CORE, ffi::GROUPING_LOCK_STATE);
            let Some(list_ref) = list.as_ref() else { return };
            for bucket_index in 0..list_ref.num_buckets as usize {
                let bucket = &*list_ref.buckets.add(bucket_index);
                for index in 0..bucket.num_achievements as usize {
                    if let Some(achievement) = (*bucket.achievements.add(index)).as_ref() {
                        action((index == 0).then_some(bucket.bucket_type), achievement);
                    }
                }
            }
            ffi::rc_client_destroy_achievement_list(list);
        }
    }

    pub fn badges(&mut self) -> &mut Badges {
        &mut self.badges
    }

    /// Waits a moment for unlocks still on their way to the server, as the game is ending.
    pub fn finish(mut self) {
        let deadline = Instant::now() + FINISH_TIMEOUT;
        while self.client.in_flight() > 0 && Instant::now() < deadline {
            self.client.pump();
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.pump();
    }
}

/// The in-game menu's ACHIEVEMENTS page: the game's achievements under their headings,
/// with one selected, whose description shows below them.
pub struct Page {
    lines: Vec<Line>,
    /// The selected line, always an achievement while there are any.
    selected: usize,
}

impl Page {
    pub fn new(lines: Vec<Line>) -> Self {
        let selected = lines.iter().position(|line| matches!(line, Line::Achievement(_))).unwrap_or(0);
        Self { lines, selected }
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        match self.lines.get(self.selected)? {
            Line::Achievement(entry) => Some(entry),
            Line::Heading(_) => None,
        }
    }

    /// The achievements' positions among the lines.
    fn achievements(&self) -> Vec<usize> {
        (0..self.lines.len()).filter(|&index| matches!(self.lines[index], Line::Achievement(_))).collect()
    }

    /// The first line on screen when `shown` fit: the lines go by pages, and the page is
    /// the selection's, or the one before when that has the heading just above it.
    pub fn first_shown(&self, shown: usize) -> usize {
        let shown = shown.max(1);
        let heading_above = self.selected > 0 && matches!(self.lines[self.selected - 1], Line::Heading(_));
        let wanted = if heading_above { self.selected - 1 } else { self.selected };
        let top = wanted / shown * shown;
        if self.selected < top + shown { top } else { self.selected / shown * shown }
    }

    /// Moves the selection. Returns whether to go back to the menu.
    pub fn press(&mut self, button: crate::input::Button, shown: usize) -> bool {
        use crate::input::Button;
        let achievements = self.achievements();
        let Some(at) = achievements.iter().position(|&index| index == self.selected) else {
            return matches!(button, Button::B | Button::Menu);
        };
        let last = achievements.len() - 1;
        let page = shown.saturating_sub(1).max(1);
        let to = match button {
            Button::Up => if at == 0 { last } else { at - 1 },
            Button::Down => if at == last { 0 } else { at + 1 },
            Button::L1 | Button::Left => at.saturating_sub(page),
            Button::R1 | Button::Right => (at + page).min(last),
            Button::B | Button::Menu => return true,
            _ => at,
        };
        self.selected = achievements[to];
        false
    }
}

/// Checks a core's options against those hardcore disallows, as `RetroArch` does: a PAL
/// region, for example, runs games slower. Returns the first that is, by its key.
pub fn disallowed_option(library_name: &str, options: &[(String, String)]) -> Option<String> {
    let name = CString::new(library_name).ok()?;
    // SAFETY: the name outlives the call, and the list is static in rcheevos.
    let disallowed = unsafe { ffi::rc_libretro_get_disallowed_settings(name.as_ptr()) };
    if disallowed.is_null() {
        return None;
    }
    options.iter().find_map(|(key, value)| {
        let (Ok(c_key), Ok(c_value)) = (CString::new(key.as_str()), CString::new(value.as_str())) else { return None };
        // SAFETY: as above.
        let allowed = unsafe { ffi::rc_libretro_is_setting_allowed(disallowed, c_key.as_ptr(), c_value.as_ptr()) != 0 };
        (!allowed).then(|| key.clone())
    })
}

/// Achievement badges: downloaded to `BADGE_DIR` while a game starts, and kept in memory
/// once drawn.
pub struct Badges {
    dir: PathBuf,
    loaded: HashMap<String, Image>,
}

impl Badges {
    pub fn new(dir: &Path) -> Self {
        Self { dir: dir.to_path_buf(), loaded: HashMap::new() }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.png"))
    }

    /// Fetches the badges not yet on the card, on a thread, one after another.
    fn download(&self, urls: Vec<(String, String)>) {
        let missing: Vec<(PathBuf, String)> = urls
            .into_iter()
            .filter(|(name, url)| !name.is_empty() && url.starts_with("https://"))
            .map(|(name, url)| (self.path(&name), url))
            .filter(|(path, _)| !path.exists())
            .collect();
        // Tests stay off the network.
        if missing.is_empty() || cfg!(test) {
            return;
        }
        let dir = self.dir.clone();
        std::thread::spawn(move || {
            use crate::boxart_fetch::Source;
            if let Err(error) = std::fs::create_dir_all(&dir) {
                eprintln!("achievements: could not make {}: {error}", dir.display());
                return;
            }
            let https = crate::boxart_fetch::Https::new();
            let mut saved = 0;
            for (path, url) in &missing {
                match https.get(url) {
                    Ok(Some(bytes)) if bytes.starts_with(b"\x89PNG") => match write_atomic(path, &bytes) {
                        Ok(()) => saved += 1,
                        Err(error) => eprintln!("achievements: could not save {}: {error}", path.display()),
                    },
                    Ok(_) => eprintln!("achievements: no badge at {url}"),
                    // No connection: the rest would fail too.
                    Err(_) => break,
                }
            }
            println!("achievements: saved {saved} of {} badges", missing.len());
        });
    }

    /// A badge at `size` pixels, grey while locked, if it has been downloaded.
    pub fn get(&mut self, name: &str, size: usize, locked: bool) -> Option<Image> {
        if !self.loaded.contains_key(name) {
            let image = crate::png::decode(&std::fs::read(self.path(name)).ok()?).ok()?;
            self.loaded.insert(name.to_string(), image);
        }
        let image = shrink(&self.loaded[name], size);
        Some(if locked { greyed(image) } else { image })
    }
}

/// An image scaled to `size` pixels square, averaging the pixels each one covers.
pub fn shrink(image: &Image, size: usize) -> Image {
    if image.width == 0 || image.height == 0 || size == 0 {
        return Image { width: size, height: size, pixels: vec![Rgb(0, 0, 0); size * size] };
    }
    let mut pixels = Vec::with_capacity(size * size);
    for y in 0..size {
        let (top, bottom) = (y * image.height / size, ((y + 1) * image.height / size).max(y * image.height / size + 1));
        for x in 0..size {
            let (left, right) = (x * image.width / size, ((x + 1) * image.width / size).max(x * image.width / size + 1));
            let (mut sum, mut count) = ([0u32; 3], 0u32);
            for row in top..bottom.min(image.height) {
                for column in left..right.min(image.width) {
                    let Rgb(r, g, b) = image.pixels[row * image.width + column];
                    sum[0] += u32::from(r);
                    sum[1] += u32::from(g);
                    sum[2] += u32::from(b);
                    count += 1;
                }
            }
            let count = count.max(1);
            pixels.push(Rgb((sum[0] / count) as u8, (sum[1] / count) as u8, (sum[2] / count) as u8));
        }
    }
    Image { width: size, height: size, pixels }
}

/// A locked badge: grey, and darker, as the site shows them.
fn greyed(mut image: Image) -> Image {
    for pixel in &mut image.pixels {
        let grey = ((u32::from(pixel.0) * 3 + u32::from(pixel.1) * 6 + u32::from(pixel.2)) / 10 * 2 / 3) as u8;
        *pixel = Rgb(grey, grey, grey);
    }
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Answers a request in place of the server: the HTTP status and body for its parameters.
    pub(super) type FakeServer = fn(&str) -> (c_int, Vec<u8>);

    thread_local! {
        pub(super) static FAKE_SERVER: std::cell::Cell<Option<FakeServer>> = const { std::cell::Cell::new(None) };
    }

    /// Answers as the server would, with a game of two achievements like rcheevos' own tests.
    fn fake_server(request: &str) -> (c_int, Vec<u8>) {
        let body = if request.contains("r=login2") {
            r#"{"Success":true,"User":"Tester","Token":"ApiToken","Score":12,"SoftcoreScore":0,"Messages":0}"#
        } else if request.contains("r=achievementsets") {
            r#"{"Success":true,"GameId":1234,"Title":"Sample Game","ConsoleId":7,"ImageIconUrl":"http://server/Images/112233.png","RichPresenceGameId":1234,"RichPresencePatch":"","Sets":[{"AchievementSetId":1111,"GameId":1234,"Title":null,"Type":"core","ImageIconUrl":"http://server/Images/112233.png","Achievements":[{"ID":5501,"Title":"Ach1","Description":"Desc1","Flags":3,"Points":5,"MemAddr":"0xH0001=3_0xH0002=7","Author":"User1","BadgeName":"00234","Created":1367266583,"Modified":1376929305},{"ID":5502,"Title":"Ach2","Description":"Desc2","Flags":3,"Points":2,"MemAddr":"0xH0001=2_0x0002=9","Author":"User1","BadgeName":"00235","Created":1376970283,"Modified":1376970283}],"Leaderboards":[]}]}"#
        } else if request.contains("r=startsession") {
            r#"{"Success":true,"Unlocks":[],"HardcoreUnlocks":[]}"#
        } else if request.contains("r=awardachievement") {
            r#"{"Success":true,"Score":17,"SoftcoreScore":5,"AchievementID":5501,"AchievementsRemaining":1}"#
        } else {
            r#"{"Success":true}"#
        };
        (200, body.as_bytes().to_vec())
    }

    /// The NES's 2 KiB of RAM, as a core shows it through `retro_get_memory_data`.
    static mut RAM: [u8; 2048] = [0; 2048];

    unsafe extern "C" fn ram_data(id: u32) -> *mut c_void {
        if id == 2 { std::ptr::addr_of_mut!(RAM).cast::<c_void>() } else { std::ptr::null_mut() }
    }

    unsafe extern "C" fn ram_size(id: u32) -> usize {
        if id == 2 { 2048 } else { 0 }
    }

    #[test]
    fn a_game_loads_and_unlocks_an_achievement_from_its_memory() {
        FAKE_SERVER.with(|fake| fake.set(Some(fake_server)));
        let dir = std::env::temp_dir().join(format!("launcher-ra-session-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rom = dir.join("Sample.nes");
        std::fs::write(&rom, vec![0x55u8; 16 * 1024]).unwrap();
        let save = dir.join("Sample.nes.srm");
        let account = Account { user: "Tester".into(), token: "ApiToken".into() };
        let memory = CoreMemory { data: ram_data, size: ram_size };
        let mut session = Session::start(&account, 7, &rom, &save, false, memory).unwrap();
        let mut notices = Vec::new();
        for _ in 0..10 {
            notices.extend(session.pump());
            session.do_frame();
        }
        assert_eq!(session.status(), &Status::Ready);
        assert!(!session.restricts(), "softcore");
        assert_eq!(notices[0].heading, "0 OF 2 ACHIEVEMENTS UNLOCKED");
        assert_eq!(notices[0].title, "Sample Game");
        assert_eq!(Summary::read(&summary_path(&save)).map(|summary| summary.total), Some(2));

        // SAFETY: only this test's thread touches the RAM, between frames.
        unsafe {
            RAM[1] = 3;
            RAM[2] = 7;
        }
        let mut notices = Vec::new();
        for _ in 0..10 {
            session.do_frame();
            notices.extend(session.pump());
        }
        let unlocked: Vec<_> = notices.iter().filter(|notice| notice.badge.is_some()).collect();
        assert_eq!(unlocked.len(), 1, "{notices:?}");
        assert_eq!(unlocked[0].heading, "ACHIEVEMENT UNLOCKED - 5 POINTS");
        assert_eq!((unlocked[0].title.as_str(), unlocked[0].badge.as_deref()), ("Ach1", Some("00234")));
        let summary = Summary::read(&summary_path(&save)).unwrap();
        assert_eq!((summary.unlocked, summary.total, summary.points, summary.points_total), (1, 2, 5, 7));
        let lines = session.lines();
        assert!(lines.iter().any(|line| matches!(line, Line::Achievement(entry) if entry.title == "Ach1" && entry.unlocked)));
        assert!(lines.iter().any(|line| matches!(line, Line::Achievement(entry) if entry.title == "Ach2" && !entry.unlocked)));
        session.finish();
        FAKE_SERVER.with(|fake| fake.set(None));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Memory only the core's map shows, as mGBA's high RAM is.
    static mut MAPPED: [u8; 2048] = [0; 2048];

    #[test]
    fn a_memory_map_given_after_the_start_is_used() {
        FAKE_SERVER.with(|fake| fake.set(Some(fake_server)));
        let dir = std::env::temp_dir().join(format!("launcher-ra-late-map-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rom = dir.join("Sample.nes");
        std::fs::write(&rom, vec![0x55u8; 16 * 1024]).unwrap();
        let account = Account { user: "Tester".into(), token: "ApiToken".into() };
        let memory = CoreMemory { data: ram_data, size: ram_size };
        let mut session = Session::start(&account, 7, &rom, &dir.join("Sample.nes.srm"), false, memory).unwrap();
        // As mGBA does in its first frame.
        let descriptor = ffi::retro_memory_descriptor {
            flags: 0,
            ptr: std::ptr::addr_of_mut!(MAPPED).cast::<c_void>(),
            offset: 0,
            start: 0,
            select: 0,
            disconnect: 0,
            len: 2048,
            addrspace: std::ptr::null(),
        };
        let map = ffi::retro_memory_map { descriptors: &raw const descriptor, num_descriptors: 1 };
        // SAFETY: the map is a retro_memory_map.
        unsafe { keep_memory_map((&raw const map).cast::<c_void>()) };
        // Achievements already true when the game loads wait to be false first.
        for _ in 0..10 {
            session.do_frame();
            let _ = session.pump();
        }
        assert_eq!(session.status(), &Status::Ready);
        // SAFETY: only this thread touches the memory, between frames.
        unsafe {
            MAPPED[1] = 3;
            MAPPED[2] = 7;
        }
        let mut notices = Vec::new();
        for _ in 0..10 {
            session.do_frame();
            notices.extend(session.pump());
        }
        assert!(notices.iter().any(|notice| notice.title == "Ach1"), "{notices:?}");
        session.finish();
        MEMORY_MAP.with(|slot| *slot.borrow_mut() = None);
        FAKE_SERVER.with(|fake| fake.set(None));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_password_login_keeps_the_token_it_gets() {
        FAKE_SERVER.with(|fake| fake.set(Some(fake_server)));
        assert_eq!(log_in("Tester", "secret"), Ok(Account { user: "Tester".into(), token: "ApiToken".into() }));
        fn refuse(_: &str) -> (c_int, Vec<u8>) {
            (401, br#"{"Success":false,"Error":"Invalid username/password combination. Please try again","Code":"invalid_credentials"}"#.to_vec())
        }
        FAKE_SERVER.with(|fake| fake.set(Some(refuse)));
        assert_eq!(log_in("Tester", "wrong"), Err("WRONG NAME OR PASSWORD".to_string()));
        FAKE_SERVER.with(|fake| fake.set(None));
    }

    #[test]
    fn every_console_with_achievements_has_an_id() {
        for system in &crate::SYSTEMS {
            assert_eq!(console_id(system.key).is_none(), system.key == "tamagotchi", "{}", system.key);
        }
        assert_eq!(console_id("gba"), Some(5));
        assert_eq!(console_id("wsc"), Some(53));
    }

    #[test]
    fn the_account_keeps_its_name_and_token() {
        let account = Account { user: "Player".into(), token: "abc123".into() };
        let dir = std::env::temp_dir().join(format!("launcher-ra-account-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("retroachievements.txt");
        account.write(&path).unwrap();
        assert_eq!(Account::read(&path), Some(account));
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        log_out(&path).unwrap();
        assert_eq!(Account::read(&path), None);
        log_out(&path).unwrap();
        assert_eq!(Account::parse("user=a\n"), None, "no token");
        assert_eq!(Account::parse("user=\ntoken=t\n"), None, "no name");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_summary_survives_its_file() {
        let summary = Summary { unlocked: 3, total: 40, points: 25, points_total: 400, hardcore: true };
        assert_eq!(Summary::parse(&summary.to_text()), Some(summary));
        assert_eq!(Summary::parse("unlocked=1\n"), None);
        assert_eq!(summary_path(Path::new("/s/Tetris.gb.srm")), Path::new("/s/Tetris.gb.srm.achievements"));
        assert!(is_summary(&summary_path(Path::new("/s/Tetris.gb.srm"))));
    }

    #[test]
    fn a_game_can_have_its_achievements_turned_off_and_on() {
        let dir = std::env::temp_dir().join(format!("launcher-ra-off-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let save = dir.join("Tetris.gb.srm");
        assert!(!is_off_for_game(&save));
        set_off_for_game(&save, true).unwrap();
        assert!(is_off_for_game(&save) && is_off_file(&off_path(&save)));
        set_off_for_game(&save, false).unwrap();
        set_off_for_game(&save, false).unwrap();
        assert!(!is_off_for_game(&save));
        set_off_for_game(&save, true).unwrap();
        remove_all_off(&dir);
        assert!(!is_off_for_game(&save));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn badges_shrink_by_averaging_and_grey_when_locked() {
        let image = Image { width: 2, height: 2, pixels: vec![Rgb(0, 0, 0), Rgb(200, 100, 0), Rgb(0, 0, 0), Rgb(200, 100, 0)] };
        let small = shrink(&image, 1);
        assert_eq!(small.pixels, vec![Rgb(100, 50, 0)]);
        assert_eq!(shrink(&image, 4).pixels.len(), 16);
        let grey = greyed(small);
        assert_eq!(grey.pixels[0].0, grey.pixels[0].1);
        assert!(grey.pixels[0].0 < 100);
    }

    fn entry(title: &str) -> Line {
        Line::Achievement(Entry {
            title: title.into(),
            description: String::new(),
            points: 5,
            unlocked: false,
            progress: String::new(),
            badge: String::new(),
        })
    }

    #[test]
    fn the_page_moves_between_achievements_past_headings_and_wraps() {
        use crate::input::Button;
        let lines = vec![Line::Heading("LOCKED"), entry("a"), entry("b"), Line::Heading("UNLOCKED"), entry("c")];
        let mut page = Page::new(lines);
        assert_eq!(page.selected_entry().unwrap().title, "a");
        assert!(!page.press(Button::Down, 4));
        assert!(!page.press(Button::Down, 4));
        assert_eq!(page.selected_entry().unwrap().title, "c");
        page.press(Button::Down, 4);
        assert_eq!(page.selected_entry().unwrap().title, "a", "wraps");
        page.press(Button::Up, 4);
        assert_eq!(page.selected_entry().unwrap().title, "c");
        page.press(Button::L1, 4);
        assert_eq!(page.selected_entry().unwrap().title, "a");
        assert!(page.press(Button::B, 4));
        let mut empty = Page::new(Vec::new());
        assert!(!empty.press(Button::Down, 4));
        assert!(empty.press(Button::B, 4));
    }

    #[test]
    fn the_page_shows_the_selection_with_its_heading() {
        use crate::input::Button;
        let mut lines: Vec<Line> = (0..5).map(|i| entry(&i.to_string())).collect();
        lines.insert(0, Line::Heading("LOCKED"));
        lines.push(Line::Heading("UNLOCKED"));
        lines.extend((5..8).map(|i| entry(&i.to_string())));
        // 0: heading, 1-5: 0-4, 6: heading, 7-9: 5-7.
        let mut page = Page::new(lines);
        assert_eq!(page.first_shown(3), 0);
        for _ in 0..4 {
            page.press(Button::Down, 3);
        }
        assert_eq!((page.selected(), page.first_shown(3)), (5, 3));
        page.press(Button::Down, 3);
        assert_eq!(page.selected(), 7);
        assert_eq!(page.first_shown(3), 6, "its heading comes with it");
    }

    #[test]
    fn hardcore_disallows_a_pal_region_but_not_the_default() {
        let options = |value: &str| vec![("snes9x_region".to_string(), value.to_string())];
        assert_eq!(disallowed_option("Snes9x", &options("pal")), Some("snes9x_region".into()));
        assert_eq!(disallowed_option("Snes9x", &options("auto")), None);
        assert_eq!(disallowed_option("mGBA", &options("pal")), None, "only that core's options");
    }

    #[test]
    fn rcheevos_hashes_a_game_file() {
        extern "C" {
            fn rc_hash_generate_from_buffer(hash: *mut c_char, console_id: u32, buffer: *const u8, size: usize) -> c_int;
        }
        let rom = vec![0u8; 32 * 1024];
        let mut hash = [0 as c_char; 33];
        // SAFETY: the hash buffer has the 33 bytes rc_hash.h asks for.
        assert_ne!(unsafe { rc_hash_generate_from_buffer(hash.as_mut_ptr(), 4, rom.as_ptr(), rom.len()) }, 0);
        // The MD5 of 32 KiB of zeros, as rcheevos hashes a Game Boy game: all of it.
        assert_eq!(field_text(&hash), "bb7df04e1b0a2570657527a7e108ae23");
    }

    /// The structs declared by hand in `ffi` have the sizes the C compiler gives them. A
    /// field added or dropped by a newer rcheevos shows here. Two are only ever read through
    /// a pointer from rcheevos and declare just their first fields, so they may be shorter.
    #[test]
    fn the_declared_structs_have_the_c_sizes() {
        macro_rules! c_size {
            ($name:ident) => {{
                extern "C" {
                    #[link_name = concat!("launcher_sizeof_", stringify!($name))]
                    static SIZE: usize;
                }
                // SAFETY: a constant in rcheevos_layout.c, which build.rs compiles in.
                unsafe { SIZE }
            }};
        }
        macro_rules! whole {
            ($($name:ident)*) => {$(assert_eq!(size_of::<ffi::$name>(), c_size!($name), stringify!($name));)*};
        }
        macro_rules! start_of {
            ($($name:ident)*) => {$(assert!(size_of::<ffi::$name>() <= c_size!($name), stringify!($name));)*};
        }
        whole!(
            rc_api_server_response_t rc_client_game_t rc_client_user_game_summary_t rc_client_achievement_t
            rc_client_achievement_bucket_t rc_client_achievement_list_t rc_client_leaderboard_t
            rc_client_server_error_t rc_client_event_t rc_libretro_memory_regions_t
            rc_libretro_core_memory_info_t retro_memory_descriptor retro_memory_map
        );
        start_of!(rc_api_request_t rc_client_user_t);
    }
}
