//! A minimal libretro frontend: loads a core, and runs one game on the framebuffer.

use crate::fb::Framebuffer;
use crate::input::{self, Button, Controls};
use std::cell::RefCell;
use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

extern "C" {
    fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    fn dlerror() -> *const c_char;
    fn fcntl(fd: c_int, command: c_int, ...) -> c_int;
}

const RTLD_NOW: c_int = 2;
const F_SETPIPE_SZ: c_int = 1031;

const ENVIRONMENT_EXPERIMENTAL: c_uint = 0x10000;
const ENVIRONMENT_GET_CAN_DUPE: c_uint = 3;
const ENVIRONMENT_GET_SYSTEM_DIRECTORY: c_uint = 9;
const ENVIRONMENT_SET_PIXEL_FORMAT: c_uint = 10;
const ENVIRONMENT_GET_VARIABLE_UPDATE: c_uint = 17;
const ENVIRONMENT_GET_SAVE_DIRECTORY: c_uint = 31;

const DEVICE_JOYPAD: c_uint = 1;
const MEMORY_SAVE_RAM: c_uint = 0;

/// Without a final save, at most this much in-game saving can be lost on power-off.
const AUTOSAVE_FRAMES: u64 = 120;
const STATS_FRAMES: u64 = 600;
const CPU_SPEED_PATH: &str = "/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq";

#[derive(Clone, Copy, PartialEq, Debug)]
enum PixelFormat {
    Rgb1555,
    Xrgb8888,
    Rgb565,
}

#[repr(C)]
struct GameInfo {
    path: *const c_char,
    data: *const c_void,
    size: usize,
    meta: *const c_char,
}

#[repr(C)]
#[derive(Default)]
struct Geometry {
    base_width: c_uint,
    base_height: c_uint,
    max_width: c_uint,
    max_height: c_uint,
    aspect_ratio: f32,
}

#[repr(C)]
#[derive(Default)]
struct Timing {
    fps: f64,
    sample_rate: f64,
}

#[repr(C)]
#[derive(Default)]
struct AvInfo {
    geometry: Geometry,
    timing: Timing,
}

/// What the C callbacks need. The core calls them on the thread that runs it.
struct Host {
    format: PixelFormat,
    directory: CString,
    frame: Vec<u32>,
    frame_size: (usize, usize),
    target: (usize, usize),
    frame_ready: bool,
    audio: Option<Child>,
    audio_bytes: u64,
    controls: Controls,
    scale_time: Duration,
    audio_time: Duration,
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

fn with_host<T>(action: impl FnOnce(&mut Host) -> T) -> T {
    HOST.with(|host| action(host.borrow_mut().as_mut().expect("host is set before the core runs")))
}

unsafe extern "C" fn environment(command: c_uint, data: *mut c_void) -> bool {
    match command & !ENVIRONMENT_EXPERIMENTAL {
        ENVIRONMENT_GET_CAN_DUPE => {
            *(data as *mut bool) = true;
            true
        }
        ENVIRONMENT_GET_SYSTEM_DIRECTORY | ENVIRONMENT_GET_SAVE_DIRECTORY => {
            let directory = with_host(|host| host.directory.as_ptr());
            *(data as *mut *const c_char) = directory;
            true
        }
        ENVIRONMENT_SET_PIXEL_FORMAT => {
            let format = match *(data as *const c_int) {
                0 => PixelFormat::Rgb1555,
                1 => PixelFormat::Xrgb8888,
                2 => PixelFormat::Rgb565,
                _ => return false,
            };
            with_host(|host| host.format = format);
            true
        }
        ENVIRONMENT_GET_VARIABLE_UPDATE => {
            *(data as *mut bool) = false;
            true
        }
        // Everything else is optional for a core.
        _ => false,
    }
}

unsafe extern "C" fn video_refresh(
    data: *const c_void,
    width: c_uint,
    height: c_uint,
    pitch: usize,
) {
    // A null frame means the previous one is repeated.
    if data.is_null() {
        return;
    }
    let (width, height) = (width as usize, height as usize);
    with_host(|host| {
        let start = Instant::now();
        let bytes_per_pixel = if host.format == PixelFormat::Xrgb8888 { 4 } else { 2 };
        // SAFETY: the core guarantees `height` rows of `pitch` bytes.
        let source = std::slice::from_raw_parts(
            data as *const u8,
            pitch * (height - 1) + width * bytes_per_pixel,
        );
        let size = fit(width, height, host.target.0, host.target.1);
        host.frame.resize(size.0 * size.1, 0);
        host.frame_size = size;
        scale_frame(source, pitch, host.format, (width, height), &mut host.frame, size);
        host.frame_ready = true;
        host.scale_time += start.elapsed();
    });
}

fn write_audio(host: &mut Host, bytes: &[u8]) {
    host.audio_bytes += bytes.len() as u64;
    let start = Instant::now();
    if let Some(stdin) = host.audio.as_mut().and_then(|child| child.stdin.as_mut()) {
        // If aplay has gone, the game keeps running silently.
        let _ = stdin.write_all(bytes);
    }
    host.audio_time += start.elapsed();
}

unsafe extern "C" fn audio_sample(left: i16, right: i16) {
    let [l0, l1] = left.to_le_bytes();
    let [r0, r1] = right.to_le_bytes();
    with_host(|host| write_audio(host, &[l0, l1, r0, r1]));
}

unsafe extern "C" fn audio_batch(data: *const i16, frames: usize) -> usize {
    // SAFETY: the core provides `frames` stereo pairs of 16-bit samples.
    let bytes = std::slice::from_raw_parts(data as *const u8, frames * 4);
    with_host(|host| write_audio(host, bytes));
    frames
}

unsafe extern "C" fn input_poll() {}

unsafe extern "C" fn input_state(port: c_uint, device: c_uint, _: c_uint, id: c_uint) -> i16 {
    if port != 0 || device != DEVICE_JOYPAD {
        return 0;
    }
    let Some(button) = joypad_button(id) else {
        return 0;
    };
    with_host(|host| i16::from(host.controls.is_held(button)))
}

/// Maps a `RETRO_DEVICE_ID_JOYPAD_*` id to the button with that label.
fn joypad_button(id: c_uint) -> Option<Button> {
    Some(match id {
        0 => Button::B,
        1 => Button::Y,
        2 => Button::Select,
        3 => Button::Start,
        4 => Button::Up,
        5 => Button::Down,
        6 => Button::Left,
        7 => Button::Right,
        8 => Button::A,
        9 => Button::X,
        10 => Button::L1,
        11 => Button::R1,
        12 => Button::L2,
        13 => Button::R2,
        _ => return None,
    })
}

/// The largest size with the source's shape that fits the target.
fn fit(source_w: usize, source_h: usize, target_w: usize, target_h: usize) -> (usize, usize) {
    if target_w * source_h <= target_h * source_w {
        (target_w, target_w * source_h / source_w)
    } else {
        (target_h * source_w / source_h, target_h)
    }
}

/// Expands 5 or 6 bits to 8, so full brightness stays full.
fn expand(value: u32, bits: u32) -> u32 {
    (value << (8 - bits)) | (value >> (2 * bits - 8))
}

fn to_xrgb(format: PixelFormat, bytes: &[u8]) -> u32 {
    match format {
        PixelFormat::Xrgb8888 => u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        PixelFormat::Rgb565 => {
            let p = u32::from(u16::from_ne_bytes([bytes[0], bytes[1]]));
            expand(p >> 11, 5) << 16 | expand((p >> 5) & 0x3F, 6) << 8 | expand(p & 0x1F, 5)
        }
        PixelFormat::Rgb1555 => {
            let p = u32::from(u16::from_ne_bytes([bytes[0], bytes[1]]));
            expand((p >> 10) & 0x1F, 5) << 16 | expand((p >> 5) & 0x1F, 5) << 8 | expand(p & 0x1F, 5)
        }
    }
}

/// Converts to XRGB8888 and scales with nearest neighbour.
fn scale_frame(
    source: &[u8],
    pitch: usize,
    format: PixelFormat,
    (source_w, source_h): (usize, usize),
    dest: &mut [u32],
    (dest_w, dest_h): (usize, usize),
) {
    let bytes_per_pixel = if format == PixelFormat::Xrgb8888 { 4 } else { 2 };
    let columns: Vec<usize> = (0..dest_w).map(|x| x * source_w / dest_w).collect();
    let mut converted = vec![0u32; source_w];
    let mut converted_row = usize::MAX;
    for (y, row) in dest.chunks_exact_mut(dest_w).enumerate() {
        let source_y = y * source_h / dest_h;
        if source_y != converted_row {
            let start = source_y * pitch;
            for (x, pixel) in converted.iter_mut().enumerate() {
                let offset = start + x * bytes_per_pixel;
                *pixel = to_xrgb(format, &source[offset..offset + bytes_per_pixel]);
            }
            converted_row = source_y;
        }
        for (pixel, &column) in row.iter_mut().zip(&columns) {
            *pixel = converted[column];
        }
    }
}

struct Core {
    run: unsafe extern "C" fn(),
    unload_game: unsafe extern "C" fn(),
    deinit: unsafe extern "C" fn(),
    memory_data: unsafe extern "C" fn(c_uint) -> *mut c_void,
    memory_size: unsafe extern "C" fn(c_uint) -> usize,
    av_info: AvInfo,
    /// The core may read from this for as long as the game is loaded.
    _rom: Vec<u8>,
}

fn last_dl_error() -> io::Error {
    // SAFETY: dlerror returns null or a valid C string.
    let message = unsafe {
        let error = dlerror();
        if error.is_null() {
            "unknown error".to_string()
        } else {
            CStr::from_ptr(error).to_string_lossy().into_owned()
        }
    };
    io::Error::other(message)
}

fn c_path(path: &Path) -> io::Result<CString> {
    use std::os::unix::ffi::OsStrExt;
    CString::new(path.as_os_str().as_bytes()).map_err(io::Error::other)
}

impl Core {
    fn load(core_path: &Path, rom_path: &Path) -> io::Result<Self> {
        let core_name = c_path(core_path)?;
        // SAFETY: the symbols are cast to the signatures in libretro.h.
        unsafe {
            let handle = dlopen(core_name.as_ptr(), RTLD_NOW);
            if handle.is_null() {
                return Err(last_dl_error());
            }
            let symbol = |name: &CStr| -> io::Result<*mut c_void> {
                let address = dlsym(handle, name.as_ptr());
                if address.is_null() {
                    return Err(last_dl_error());
                }
                Ok(address)
            };
            macro_rules! function {
                ($name:literal, $type:ty) => {
                    std::mem::transmute::<*mut c_void, $type>(symbol($name)?)
                };
            }
            type SetCallback = unsafe extern "C" fn(*const c_void);

            function!(c"retro_set_environment", SetCallback)(environment as *const c_void);
            function!(c"retro_set_video_refresh", SetCallback)(video_refresh as *const c_void);
            function!(c"retro_set_audio_sample", SetCallback)(audio_sample as *const c_void);
            function!(c"retro_set_audio_sample_batch", SetCallback)(audio_batch as *const c_void);
            function!(c"retro_set_input_poll", SetCallback)(input_poll as *const c_void);
            function!(c"retro_set_input_state", SetCallback)(input_state as *const c_void);
            function!(c"retro_init", unsafe extern "C" fn())();

            let rom = std::fs::read(rom_path)?;
            let rom_name = c_path(rom_path)?;
            let game = GameInfo {
                path: rom_name.as_ptr(),
                data: rom.as_ptr() as *const c_void,
                size: rom.len(),
                meta: std::ptr::null(),
            };
            if !function!(c"retro_load_game", unsafe extern "C" fn(*const GameInfo) -> bool)(&game) {
                return Err(io::Error::other("the core could not load this game"));
            }
            let mut av_info = AvInfo::default();
            function!(c"retro_get_system_av_info", unsafe extern "C" fn(*mut AvInfo))(&mut av_info);
            Ok(Self {
                run: function!(c"retro_run", unsafe extern "C" fn()),
                unload_game: function!(c"retro_unload_game", unsafe extern "C" fn()),
                deinit: function!(c"retro_deinit", unsafe extern "C" fn()),
                memory_data: function!(c"retro_get_memory_data", unsafe extern "C" fn(c_uint) -> *mut c_void),
                memory_size: function!(c"retro_get_memory_size", unsafe extern "C" fn(c_uint) -> usize),
                av_info,
                _rom: rom,
            })
        }
    }

    /// The game's battery-backed memory. Empty if it has none.
    fn save_ram(&mut self) -> &mut [u8] {
        // SAFETY: the core keeps this memory valid while the game is loaded.
        unsafe {
            let data = (self.memory_data)(MEMORY_SAVE_RAM) as *mut u8;
            let size = (self.memory_size)(MEMORY_SAVE_RAM);
            if data.is_null() || size == 0 {
                return &mut [];
            }
            std::slice::from_raw_parts_mut(data, size)
        }
    }
}

/// Keeps a save file in step with the game's battery-backed memory.
struct SaveFile {
    path: PathBuf,
    written: Vec<u8>,
}

impl SaveFile {
    fn load_into(path: PathBuf, save_ram: &mut [u8]) -> Self {
        if let Ok(saved) = std::fs::read(&path) {
            let size = saved.len().min(save_ram.len());
            save_ram[..size].copy_from_slice(&saved[..size]);
        }
        Self { written: save_ram.to_vec(), path }
    }

    fn update(&mut self, save_ram: &[u8]) -> io::Result<()> {
        if self.written == save_ram {
            return Ok(());
        }
        // The old save survives a power cut during the write.
        let temporary = self.path.with_extension("tmp");
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(save_ram)?;
        file.sync_all()?;
        std::fs::rename(&temporary, &self.path)?;
        self.written = save_ram.to_vec();
        Ok(())
    }
}

fn set_host(directory: &Path, target: (usize, usize)) -> io::Result<()> {
    let host = Host {
        format: PixelFormat::Rgb1555,
        directory: c_path(directory)?,
        frame: Vec::new(),
        frame_size: (0, 0),
        target,
        frame_ready: false,
        audio: None,
        audio_bytes: 0,
        controls: Controls::new(),
        scale_time: Duration::ZERO,
        audio_time: Duration::ZERO,
    };
    HOST.with(|slot| *slot.borrow_mut() = Some(host));
    Ok(())
}

/// Plays through aplay. A small pipe keeps latency low, and paces the game to the
/// audio hardware, because writes wait while the pipe is full.
fn start_audio(sample_rate: f64) -> io::Result<Child> {
    let child = Command::new("aplay")
        .args(["-q", "-t", "raw", "-f", "S16_LE", "-c", "2"])
        .args(["-r", &format!("{}", sample_rate.round() as u32)])
        .args(["--buffer-time=100000", "--period-time=25000"])
        .stdin(Stdio::piped())
        .spawn()?;
    if let Some(stdin) = &child.stdin {
        // SAFETY: F_SETPIPE_SZ takes its size by value. A failure only adds latency.
        unsafe { fcntl(stdin.as_raw_fd(), F_SETPIPE_SZ, 4096 as c_int) };
    }
    Ok(child)
}

fn wants_to_leave(controls: &Controls) -> bool {
    controls.is_held(Button::Menu) && controls.is_held(Button::Start)
}

/// Runs a game on the framebuffer until Menu and Start are held together.
pub fn play(core_path: &Path, rom: &Path, save: &Path, framebuffer: &mut Framebuffer) -> io::Result<()> {
    let directory = save.parent().unwrap_or(Path::new("."));
    set_host(directory, framebuffer.size())?;
    let mut core = Core::load(core_path, rom)?;
    let mut save_file = SaveFile::load_into(save.to_path_buf(), core.save_ram());
    let (fps, sample_rate) = (core.av_info.timing.fps, core.av_info.timing.sample_rate);
    println!("playing {} at {fps:.2} fps, {sample_rate:.0} Hz", rom.display());

    match start_audio(sample_rate) {
        Ok(audio) => with_host(|host| host.audio = Some(audio)),
        Err(error) => eprintln!("warning: no audio: {error}"),
    }
    let frame_time = Duration::from_secs_f64(1.0 / fps);
    let events = input::spawn_readers();
    framebuffer.clear()?;

    let mut next_frame = Instant::now();
    let mut stats_start = next_frame;
    let mut run_time = Duration::ZERO;
    let mut screen_time = Duration::ZERO;
    let mut vsync_time = Duration::ZERO;
    let mut vsync_works = true;
    let mut frames = 0u64;
    loop {
        let leave = with_host(|host| {
            for event in events.try_iter() {
                host.controls.apply(&event);
            }
            wants_to_leave(&host.controls)
        });
        if leave {
            break;
        }
        // SAFETY: the game is loaded, and the callbacks run on this thread.
        let run_start = Instant::now();
        unsafe { (core.run)() };
        run_time += run_start.elapsed();
        with_host(|host| -> io::Result<()> {
            if host.frame_ready {
                if vsync_works {
                    let vsync_start = Instant::now();
                    if let Err(error) = framebuffer.wait_for_vsync() {
                        eprintln!("warning: can't wait for the panel, so the game may tear: {error}");
                        vsync_works = false;
                    }
                    vsync_time += vsync_start.elapsed();
                }
                let screen_start = Instant::now();
                let (width, height) = host.frame_size;
                framebuffer.blit_centred(&host.frame, width, height)?;
                host.frame_ready = false;
                screen_time += screen_start.elapsed();
            }
            Ok(())
        })?;

        frames += 1;
        if frames.is_multiple_of(AUTOSAVE_FRAMES) {
            if let Err(error) = save_file.update(core.save_ram()) {
                eprintln!("warning: could not save: {error}");
            }
        }
        if frames.is_multiple_of(STATS_FRAMES) {
            let fps = STATS_FRAMES as f64 / stats_start.elapsed().as_secs_f64();
            let (scale, audio) = with_host(|host| {
                (std::mem::take(&mut host.scale_time), std::mem::take(&mut host.audio_time))
            });
            let per_frame = |time: Duration| time.as_secs_f64() * 1000.0 / STATS_FRAMES as f64;
            let emulate = run_time.saturating_sub(scale + audio);
            let speed = std::fs::read_to_string(CPU_SPEED_PATH).unwrap_or_default();
            println!(
                "{fps:.2} fps, ms per frame: emulate {:.1}, scale {:.1}, screen {:.1}, audio wait {:.1}, vsync wait {:.1}, cpu {} kHz",
                per_frame(emulate),
                per_frame(scale),
                per_frame(screen_time),
                per_frame(audio),
                per_frame(vsync_time),
                speed.trim(),
            );
            run_time = Duration::ZERO;
            screen_time = Duration::ZERO;
            vsync_time = Duration::ZERO;
            stats_start = Instant::now();
        }

        // The panel and audio pace the game. This only covers them both failing.
        if !vsync_works {
            next_frame += frame_time;
            let now = Instant::now();
            if next_frame > now {
                sleep(next_frame - now);
            } else if now - next_frame > Duration::from_millis(100) {
                next_frame = now;
            }
        }
    }

    save_file.update(core.save_ram())?;
    let audio = with_host(|host| host.audio.take());
    if let Some(mut audio) = audio {
        drop(audio.stdin.take());
        let _ = audio.wait();
    }
    // SAFETY: the game is loaded, and nothing uses the core after this.
    unsafe {
        (core.unload_game)();
        (core.deinit)();
    }
    Ok(())
}

/// Runs a game with no display, audio or pacing, to check a core away from the device.
/// Writes the last frame as a PPM image.
pub fn render(core_path: &Path, rom: &Path, save: &Path, frames: u64, output: &Path) -> io::Result<()> {
    set_host(save.parent().unwrap_or(Path::new(".")), (640, 480))?;
    let mut core = Core::load(core_path, rom)?;
    let mut save_file = SaveFile::load_into(save.to_path_buf(), core.save_ram());
    let start = Instant::now();
    for _ in 0..frames {
        // SAFETY: the game is loaded, and the callbacks run on this thread.
        unsafe { (core.run)() };
    }
    let elapsed = start.elapsed().as_secs_f64();
    save_file.update(core.save_ram())?;

    let (bytes, width, height, pixels) = with_host(|host| {
        (host.audio_bytes, host.frame_size.0, host.frame_size.1, std::mem::take(&mut host.frame))
    });
    println!("{frames} frames in {elapsed:.2} s: {:.0} fps", frames as f64 / elapsed);
    println!("core timing: {:.2} fps, {:.0} Hz", core.av_info.timing.fps, core.av_info.timing.sample_rate);
    println!("audio: {bytes} bytes, save RAM: {} bytes", core.save_ram().len());

    let mut file = io::BufWriter::new(std::fs::File::create(output)?);
    write!(file, "P6\n{width} {height}\n255\n")?;
    for pixel in pixels {
        file.write_all(&[(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8])?;
    }
    file.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gba_screen_fills_the_panel_width() {
        assert_eq!(fit(240, 160, 640, 480), (640, 426));
    }

    #[test]
    fn a_tall_source_fits_the_panel_height() {
        assert_eq!(fit(100, 200, 640, 480), (240, 480));
    }

    #[test]
    fn rgb565_white_stays_white() {
        assert_eq!(to_xrgb(PixelFormat::Rgb565, &0xFFFFu16.to_ne_bytes()), 0x00FF_FFFF);
    }

    #[test]
    fn rgb565_channels_land_in_place() {
        assert_eq!(to_xrgb(PixelFormat::Rgb565, &0xF800u16.to_ne_bytes()), 0x00FF_0000);
        assert_eq!(to_xrgb(PixelFormat::Rgb565, &0x07E0u16.to_ne_bytes()), 0x0000_FF00);
        assert_eq!(to_xrgb(PixelFormat::Rgb565, &0x001Fu16.to_ne_bytes()), 0x0000_00FF);
    }

    #[test]
    fn scaling_uses_the_pitch_and_keeps_every_corner() {
        // Two rows of two RGB565 pixels, with two bytes of padding per row.
        let mut source = Vec::new();
        for pixel in [0xF800u16, 0x07E0, 0, 0x001F, 0xFFFF, 0] {
            source.extend_from_slice(&pixel.to_ne_bytes());
        }
        let mut dest = vec![0; 16];
        scale_frame(&source, 6, PixelFormat::Rgb565, (2, 2), &mut dest, (4, 4));
        assert_eq!(dest[0], 0x00FF_0000);
        assert_eq!(dest[3], 0x0000_FF00);
        assert_eq!(dest[12], 0x0000_00FF);
        assert_eq!(dest[15], 0x00FF_FFFF);
    }

    #[test]
    fn joypad_ids_use_the_labels_on_the_buttons() {
        assert_eq!(joypad_button(0), Some(Button::B));
        assert_eq!(joypad_button(8), Some(Button::A));
        assert_eq!(joypad_button(10), Some(Button::L1));
        assert_eq!(joypad_button(11), Some(Button::R1));
        assert_eq!(joypad_button(99), None);
    }

    #[test]
    fn a_save_is_only_written_when_it_changes() {
        let dir = std::env::temp_dir().join(format!("welcome-save-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("game.srm");
        let mut ram = vec![1u8; 8];

        let mut save = SaveFile::load_into(path.clone(), &mut ram);
        save.update(&ram).unwrap();
        assert!(!path.exists());

        ram[0] = 2;
        save.update(&ram).unwrap();
        assert_eq!(std::fs::read(&path).unwrap()[0], 2);

        let mut fresh = vec![0u8; 8];
        SaveFile::load_into(path, &mut fresh);
        assert_eq!(fresh[0], 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
