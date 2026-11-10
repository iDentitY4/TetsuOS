//! Minimal Linux framebuffer access, through its ioctls.

use crate::canvas::Canvas;
use crate::color::Rgb;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::FileExt;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use libc::{c_ulong, ioctl, Ioctl};

/// Set once the screen shows the last thing before a power-off or reboot, so nothing this
/// process draws afterwards, like a game's next frame, replaces it. Held during each write,
/// so a write that started before it was set finishes first.
static LAST_SCREEN_SHOWN: Mutex<bool> = Mutex::new(false);

// From linux/fb.h and linux/kd.h, which the libc crate doesn't have.
const FBIOGET_VSCREENINFO: Ioctl = 0x4600;
const FBIOGET_FSCREENINFO: Ioctl = 0x4602;
const FBIO_WAITFORVSYNC: Ioctl = 0x4004_4620;
const KDSETMODE: Ioctl = 0x4B3A;
const KD_GRAPHICS: c_ulong = 1;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Bitfield {
    offset: u32,
    length: u32,
    msb_right: u32,
}

/// `struct fb_var_screeninfo` from linux/fb.h.
#[repr(C)]
#[derive(Default)]
struct VarScreenInfo {
    xres: u32,
    yres: u32,
    xres_virtual: u32,
    yres_virtual: u32,
    xoffset: u32,
    yoffset: u32,
    bits_per_pixel: u32,
    grayscale: u32,
    red: Bitfield,
    green: Bitfield,
    blue: Bitfield,
    transp: Bitfield,
    rest: [u32; 20],
}

/// `struct fb_fix_screeninfo` from linux/fb.h.
#[repr(C)]
#[derive(Default)]
struct FixScreenInfo {
    id: [u8; 16],
    smem_start: c_ulong,
    smem_len: u32,
    fb_type: u32,
    type_aux: u32,
    visual: u32,
    xpanstep: u16,
    ypanstep: u16,
    ywrapstep: u16,
    line_length: u32,
    mmio_start: c_ulong,
    mmio_len: u32,
    accel: u32,
    capabilities: u16,
    reserved: [u16; 2],
}

/// Scales an 8-bit channel to its field in the pixel format.
fn pack_channel(value: u8, field: Bitfield) -> u32 {
    if field.length >= 8 {
        u32::from(value) << (field.offset + field.length - 8)
    } else {
        (u32::from(value) >> (8 - field.length)) << field.offset
    }
}

fn pack(color: Rgb, red: Bitfield, green: Bitfield, blue: Bitfield) -> u32 {
    pack_channel(color.0, red) | pack_channel(color.1, green) | pack_channel(color.2, blue)
}

/// A part of the screen: left, top, width and height.
pub type Area = (usize, usize, usize, usize);

/// Something shown on top of whatever else is drawn, for a moment.
struct Sign {
    pixels: Vec<u32>,
    area: Area,
    until: Instant,
}

/// The columns of a row, starting at `left` and `width` long, that an area covers on
/// screen line `y`, counted from the row's start.
fn covered_columns((x, top, w, h): Area, left: usize, y: usize, width: usize) -> Option<(usize, usize)> {
    let start = x.max(left);
    let end = (x + w).min(left + width);
    (y >= top && y < top + h && start < end).then(|| (start - left, end - left))
}

fn xrgb_pixels(canvas: &Canvas) -> Vec<u32> {
    canvas
        .pixels
        .iter()
        .map(|color| u32::from(color.0) << 16 | u32::from(color.1) << 8 | u32::from(color.2))
        .collect()
}

pub struct Framebuffer {
    file: File,
    var: VarScreenInfo,
    line_length: usize,
    sign: Option<Sign>,
}

impl Framebuffer {
    pub fn open(path: &str) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let mut var = VarScreenInfo::default();
        let mut fix = FixScreenInfo::default();
        // SAFETY: both structs match the kernel layouts, and outlive the calls.
        unsafe {
            if ioctl(file.as_raw_fd(), FBIOGET_VSCREENINFO, &raw mut var) != 0 {
                return Err(io::Error::last_os_error());
            }
            if ioctl(file.as_raw_fd(), FBIOGET_FSCREENINFO, &raw mut fix) != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        if !matches!(var.bits_per_pixel, 16 | 24 | 32) {
            return Err(io::Error::other(format!(
                "unsupported pixel depth: {} bpp",
                var.bits_per_pixel
            )));
        }
        Ok(Self {
            file,
            line_length: fix.line_length as usize,
            var,
            sign: None,
        })
    }

    pub fn size(&self) -> (usize, usize) {
        (self.var.xres as usize, self.var.yres as usize)
    }

    /// The byte offset of the visible screen in the framebuffer.
    fn screen_offset(&self) -> u64 {
        u64::from(self.var.yoffset) * self.line_length as u64
    }

    pub fn clear(&mut self) -> io::Result<()> {
        let buffer = vec![0u8; self.line_length * self.var.yres as usize];
        self.write_at(&buffer, self.screen_offset())?;
        self.write_sign()
    }

    /// Shows a sign centred at the top of the screen, in place of any other, until `time`
    /// has passed. Everything drawn until then leaves it showing, so a game's picture
    /// doesn't flicker over it. Whoever draws has to replace it when `sign_wait` runs out:
    /// the next `draw` leaves it out, and `take_due_sign` gives the area to fill in.
    pub fn show_sign(&mut self, canvas: &Canvas, top: usize, time: Duration) -> io::Result<()> {
        let left = self.size().0.saturating_sub(canvas.width) / 2;
        self.sign = Some(Sign {
            pixels: xrgb_pixels(canvas),
            area: (left, top, canvas.width, canvas.height),
            until: Instant::now() + time,
        });
        self.write_sign()
    }

    /// How long until the sign should go: zero once it should, and for ever without one.
    pub fn sign_wait(&self) -> Duration {
        self.sign.as_ref().map_or(Duration::MAX, |sign| sign.until.saturating_duration_since(Instant::now()))
    }

    /// Ends a sign whose time is up, and returns the area it covered.
    pub fn take_due_sign(&mut self) -> Option<Area> {
        if !self.sign_wait().is_zero() {
            return None;
        }
        self.sign.take().map(|sign| sign.area)
    }

    fn write_sign(&mut self) -> io::Result<()> {
        // Taken out, as `blit` leaves the sign's area alone.
        let Some(sign) = self.sign.take() else { return Ok(()) };
        let (left, top, width, height) = sign.area;
        let result = self.blit(&sign.pixels, width, height, left, top);
        self.sign = Some(sign);
        result
    }

    /// Writes a canvas with its top left corner at `left`, `top`.
    pub fn blit_canvas(&mut self, canvas: &Canvas, left: usize, top: usize) -> io::Result<()> {
        self.blit(&xrgb_pixels(canvas), canvas.width, canvas.height, left, top)
    }

    /// Writes XRGB8888 pixels to the middle of the screen, leaving the rest alone.
    pub fn blit_centred(&mut self, pixels: &[u32], width: usize, height: usize) -> io::Result<()> {
        let (screen_w, screen_h) = self.size();
        let left = screen_w.saturating_sub(width) / 2;
        let top = screen_h.saturating_sub(height) / 2;
        self.blit(pixels, width, height, left, top)
    }

    /// Writes XRGB8888 pixels with their top left corner at `left`, `top`, clipped to the
    /// screen, leaving the rest and any sign alone.
    pub fn blit(&mut self, pixels: &[u32], width: usize, height: usize, left: usize, top: usize) -> io::Result<()> {
        let (screen_w, screen_h) = self.size();
        if left >= screen_w || top >= screen_h || width == 0 {
            return Ok(());
        }
        let shown = width.min(screen_w - left);
        let sign = self.sign.as_ref().map(|sign| sign.area);
        for (y, row) in pixels.chunks_exact(width).take(height.min(screen_h - top)).enumerate() {
            let (row, y) = (&row[..shown], top + y);
            match sign.and_then(|area| covered_columns(area, left, y, shown)) {
                Some((start, end)) => {
                    self.write_row(&row[..start], left, y)?;
                    self.write_row(&row[end..], left + end, y)?;
                }
                None => self.write_row(row, left, y)?,
            }
        }
        Ok(())
    }

    fn write_row(&self, row: &[u32], x: usize, y: usize) -> io::Result<()> {
        let bytes_per_pixel = self.var.bits_per_pixel as usize / 8;
        let offset = self.screen_offset() + (y * self.line_length + x * bytes_per_pixel) as u64;
        let same_layout = cfg!(target_endian = "little")
            && bytes_per_pixel == 4
            && (self.var.red.offset, self.var.green.offset, self.var.blue.offset) == (16, 8, 0);
        if same_layout {
            // SAFETY: the bytes of a u32 are always valid. The check above makes sure this
            // machine's byte order is the format's.
            let bytes = unsafe { std::slice::from_raw_parts(row.as_ptr().cast::<u8>(), row.len() * 4) };
            return self.write_at(bytes, offset);
        }
        let mut buffer = vec![0u8; row.len() * bytes_per_pixel];
        for (bytes, pixel) in buffer.chunks_exact_mut(bytes_per_pixel).zip(row) {
            let color = Rgb((pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8);
            let value = pack(color, self.var.red, self.var.green, self.var.blue);
            bytes.copy_from_slice(&value.to_le_bytes()[..bytes_per_pixel]);
        }
        self.write_at(&buffer, offset)
    }

    /// Waits for the panel to finish a refresh. A full-screen write that starts right
    /// after this stays ahead of the next refresh, so it doesn't tear.
    pub fn wait_for_vsync(&self) -> io::Result<()> {
        let crtc: u32 = 0;
        // SAFETY: the request reads one u32, which outlives the call.
        if unsafe { ioctl(self.file.as_raw_fd(), FBIO_WAITFORVSYNC, &raw const crtc) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn draw(&mut self, canvas: &Canvas) -> io::Result<()> {
        if self.sign_wait().is_zero() {
            self.sign = None;
        }
        let buffer = self.screen_bytes(canvas);
        self.write_at(&buffer, self.screen_offset())
    }

    /// A full-screen canvas in the framebuffer's format, with the sign on top.
    fn screen_bytes(&self, canvas: &Canvas) -> Vec<u8> {
        let bytes_per_pixel = self.var.bits_per_pixel as usize / 8;
        let mut buffer = vec![0u8; self.line_length * canvas.height];
        for (y, row) in canvas.pixels.chunks_exact(canvas.width).enumerate() {
            for (x, color) in row.iter().enumerate() {
                let value = pack(*color, self.var.red, self.var.green, self.var.blue);
                let start = y * self.line_length + x * bytes_per_pixel;
                buffer[start..start + bytes_per_pixel]
                    .copy_from_slice(&value.to_le_bytes()[..bytes_per_pixel]);
            }
        }
        if let Some(Sign { pixels, area: (left, top, width, _), .. }) = &self.sign {
            for (y, row) in pixels.chunks_exact(*width).enumerate() {
                for (x, pixel) in row.iter().enumerate() {
                    let color = Rgb((pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8);
                    let value = pack(color, self.var.red, self.var.green, self.var.blue);
                    let start = (top + y) * self.line_length + (left + x) * bytes_per_pixel;
                    buffer[start..start + bytes_per_pixel]
                        .copy_from_slice(&value.to_le_bytes()[..bytes_per_pixel]);
                }
            }
        }
        buffer
    }

    /// Draws the last screen before a power-off or reboot, which stays until the device is
    /// off: anything drawn after it is left out. Only the first one shows.
    pub fn draw_last(&mut self, canvas: &Canvas) -> io::Result<()> {
        self.sign = None;
        let buffer = self.screen_bytes(canvas);
        let mut shown = LAST_SCREEN_SHOWN.lock().unwrap_or_else(PoisonError::into_inner);
        if *shown {
            return Ok(());
        }
        *shown = true;
        self.file.write_all_at(&buffer, self.screen_offset())
    }

    fn write_at(&self, bytes: &[u8], offset: u64) -> io::Result<()> {
        let shown = LAST_SCREEN_SHOWN.lock().unwrap_or_else(PoisonError::into_inner);
        if *shown {
            return Ok(());
        }
        self.file.write_all_at(bytes, offset)
    }

    /// Draws only part of a full-screen canvas, for small changes like a scrolling name,
    /// which are all below where signs show.
    pub fn draw_area(&mut self, canvas: &Canvas, (x, y, w, h): (usize, usize, usize, usize)) -> io::Result<()> {
        let bytes_per_pixel = self.var.bits_per_pixel as usize / 8;
        let w = w.min(canvas.width.saturating_sub(x));
        let mut buffer = vec![0u8; w * bytes_per_pixel];
        for row in y..(y + h).min(canvas.height) {
            let start = row * canvas.width + x;
            for (bytes, color) in buffer.chunks_exact_mut(bytes_per_pixel).zip(&canvas.pixels[start..start + w]) {
                let value = pack(*color, self.var.red, self.var.green, self.var.blue);
                bytes.copy_from_slice(&value.to_le_bytes()[..bytes_per_pixel]);
            }
            let offset = self.screen_offset() + (row * self.line_length + x * bytes_per_pixel) as u64;
            self.write_at(&buffer, offset)?;
        }
        Ok(())
    }
}

/// Stops the text console drawing over the screen. Lasts while the returned file is open.
pub fn claim_console() -> io::Result<File> {
    let tty = OpenOptions::new().read(true).write(true).open("/dev/tty0")?;
    // SAFETY: KDSETMODE takes its argument by value.
    if unsafe { ioctl(tty.as_raw_fd(), KDSETMODE, KD_GRAPHICS) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(tty)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(offset: u32, length: u32) -> Bitfield {
        Bitfield { offset, length, msb_right: 0 }
    }

    #[test]
    fn kernel_struct_sizes_match() {
        assert_eq!(std::mem::size_of::<VarScreenInfo>(), 160);
        assert_eq!(std::mem::size_of::<FixScreenInfo>(), 80);
    }

    #[test]
    fn a_row_is_only_covered_where_it_meets_the_area() {
        let sign = (200, 6, 240, 20);
        assert_eq!(covered_columns(sign, 0, 6, 640), Some((200, 440)));
        assert_eq!(covered_columns(sign, 0, 25, 640), Some((200, 440)));
        assert_eq!(covered_columns(sign, 0, 26, 640), None, "below it");
        assert_eq!(covered_columns(sign, 0, 5, 640), None, "above it");
        // A picture narrower than the sign, or starting inside it.
        assert_eq!(covered_columns(sign, 256, 10, 128), Some((0, 128)));
        assert_eq!(covered_columns(sign, 300, 10, 340), Some((0, 140)));
        assert_eq!(covered_columns(sign, 440, 10, 200), None, "beside it");
    }

    #[test]
    fn packs_xrgb8888() {
        let value = pack(Rgb(0x12, 0x34, 0x56), field(16, 8), field(8, 8), field(0, 8));
        assert_eq!(value, 0x0012_3456);
    }

    #[test]
    fn packs_rgb565() {
        let value = pack(Rgb(0xFF, 0x00, 0xFF), field(11, 5), field(5, 6), field(0, 5));
        assert_eq!(value, 0xF81F);
    }
}
