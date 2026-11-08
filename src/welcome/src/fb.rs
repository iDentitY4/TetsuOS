//! Minimal Linux framebuffer access, using the libc that std already links.

use crate::canvas::{Canvas, Rgb};
use std::ffi::{c_int, c_ulong};
use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::FileExt;

extern "C" {
    fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
}

const FBIOGET_VSCREENINFO: c_ulong = 0x4600;
const FBIOGET_FSCREENINFO: c_ulong = 0x4602;
const FBIO_WAITFORVSYNC: c_ulong = 0x4004_4620;
const KDSETMODE: c_ulong = 0x4B3A;
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

pub struct Framebuffer {
    file: File,
    var: VarScreenInfo,
    line_length: usize,
}

impl Framebuffer {
    pub fn open(path: &str) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let mut var = VarScreenInfo::default();
        let mut fix = FixScreenInfo::default();
        // SAFETY: both structs match the kernel layouts, and outlive the calls.
        unsafe {
            if ioctl(file.as_raw_fd(), FBIOGET_VSCREENINFO, &mut var as *mut VarScreenInfo) != 0 {
                return Err(io::Error::last_os_error());
            }
            if ioctl(file.as_raw_fd(), FBIOGET_FSCREENINFO, &mut fix as *mut FixScreenInfo) != 0 {
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
        self.file.write_all_at(&buffer, self.screen_offset())
    }

    /// Writes XRGB8888 pixels to the middle of the screen, leaving the rest alone.
    pub fn blit_centred(&mut self, pixels: &[u32], width: usize, height: usize) -> io::Result<()> {
        let (screen_w, screen_h) = self.size();
        let left = screen_w.saturating_sub(width) / 2;
        let top = screen_h.saturating_sub(height) / 2;
        self.blit(pixels, width, height, left, top)
    }

    /// Writes XRGB8888 pixels with their top left corner at `left`, `top`, clipped to the
    /// screen, leaving the rest alone.
    pub fn blit(&mut self, pixels: &[u32], width: usize, height: usize, left: usize, top: usize) -> io::Result<()> {
        let (screen_w, screen_h) = self.size();
        if left >= screen_w || top >= screen_h || width == 0 {
            return Ok(());
        }
        let bytes_per_pixel = self.var.bits_per_pixel as usize / 8;
        let row_bytes = width.min(screen_w - left) * bytes_per_pixel;
        let left = left * bytes_per_pixel;
        let is_xrgb = bytes_per_pixel == 4
            && (self.var.red.offset, self.var.green.offset, self.var.blue.offset) == (16, 8, 0);

        let mut row_buffer = vec![0u8; if is_xrgb { 0 } else { row_bytes }];
        for (y, row) in pixels.chunks_exact(width).take(height.min(screen_h - top)).enumerate() {
            let offset = self.screen_offset() + ((top + y) * self.line_length + left) as u64;
            if is_xrgb {
                // SAFETY: the bytes of a u32 are always valid, and row_bytes is within the
                // row. The device's arm64 and this format are both little-endian.
                let bytes = unsafe { std::slice::from_raw_parts(row.as_ptr() as *const u8, row_bytes) };
                self.file.write_all_at(bytes, offset)?;
            } else {
                for (bytes, pixel) in row_buffer.chunks_exact_mut(bytes_per_pixel).zip(row) {
                    let color = Rgb((pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8);
                    let value = pack(color, self.var.red, self.var.green, self.var.blue);
                    bytes.copy_from_slice(&value.to_le_bytes()[..bytes_per_pixel]);
                }
                self.file.write_all_at(&row_buffer, offset)?;
            }
        }
        Ok(())
    }

    /// Waits for the panel to finish a refresh. A full-screen write that starts right
    /// after this stays ahead of the next refresh, so it doesn't tear.
    pub fn wait_for_vsync(&self) -> io::Result<()> {
        let crtc: u32 = 0;
        // SAFETY: the request reads one u32, which outlives the call.
        if unsafe { ioctl(self.file.as_raw_fd(), FBIO_WAITFORVSYNC, &crtc as *const u32) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn draw(&mut self, canvas: &Canvas) -> io::Result<()> {
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
        let offset = u64::from(self.var.yoffset) * self.line_length as u64;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(&buffer)
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
