//! The panel's refresh rate during a game. The panel runs at 59.94 Hz, where a 50 Hz
//! European game shows some frames twice. For such a game, the `--play` process takes
//! the display (`/dev/dri/card1`) and sets a mode with more pixels per frame at the same
//! 24 MHz pixel clock, so the panel refreshes at the game's rate. The framebuffer stays
//! the one the kernel's fbdev emulation made, so drawing into `/dev/fb0` goes on as before.
//! Closing the device, also when the process ends or crashes, makes the kernel set its
//! own 59.94 Hz mode again. Each change turns the panel off and on, about 160 ms.
//! While this process holds the display, the kernel refuses `FBIO_WAITFORVSYNC` (EBUSY),
//! so the game waits for the refresh through the display instead (`wait_for_vblank`).

use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;

use libc::{ioctl, Ioctl};

/// The display: Panfrost is `card0`.
const DISPLAY: &str = "/dev/dri/card1";

/// The panel's rate in its own mode, which paces games.
pub const PANEL_HZ: f64 = 59.94;

/// Games between these rates get a matching panel rate: the 50 Hz consoles.
const MATCHED_FPS: std::ops::RangeInclusive<f64> = 45.0..=55.0;

/// How far the timing may grow from the panel's own 770 x 520 pixels per frame. Only
/// the blank space after each line and frame grows, which the panel takes from DE.
const MAX_HTOTAL: u16 = 800;
const MAX_VTOTAL: u16 = 700;

// From drm/drm.h and drm/drm_mode.h: _IO and _IOWR on 'd' with each struct's size.
const DRM_IOCTL_SET_MASTER: Ioctl = 0x641E;
const DRM_IOCTL_WAIT_VBLANK: Ioctl = 0xC018_643A;
const DRM_VBLANK_RELATIVE: u32 = 1;
const DRM_IOCTL_MODE_GETRESOURCES: Ioctl = 0xC040_64A0;
const DRM_IOCTL_MODE_GETCRTC: Ioctl = 0xC068_64A1;
const DRM_IOCTL_MODE_SETCRTC: Ioctl = 0xC068_64A2;
const DRM_IOCTL_MODE_GETENCODER: Ioctl = 0xC014_64A6;
const DRM_IOCTL_MODE_GETCONNECTOR: Ioctl = 0xC050_64A7;
const DRM_MODE_TYPE_USERDEF: u32 = 1 << 5;

/// `struct drm_mode_card_res`.
#[repr(C)]
#[derive(Default)]
struct CardResources {
    fb_id_ptr: u64,
    crtc_id_ptr: u64,
    connector_id_ptr: u64,
    encoder_id_ptr: u64,
    count_fbs: u32,
    count_crtcs: u32,
    count_connectors: u32,
    count_encoders: u32,
    min_width: u32,
    max_width: u32,
    min_height: u32,
    max_height: u32,
}

/// `struct drm_mode_modeinfo`.
#[repr(C)]
#[derive(Default, Clone, Copy, Debug)]
struct ModeInfo {
    clock: u32,
    hdisplay: u16,
    hsync_start: u16,
    hsync_end: u16,
    htotal: u16,
    hskew: u16,
    vdisplay: u16,
    vsync_start: u16,
    vsync_end: u16,
    vtotal: u16,
    vscan: u16,
    vrefresh: u32,
    flags: u32,
    mode_type: u32,
    name: [u8; 32],
}

/// `struct drm_mode_crtc`.
#[repr(C)]
#[derive(Default)]
struct Crtc {
    set_connectors_ptr: u64,
    count_connectors: u32,
    crtc_id: u32,
    fb_id: u32,
    x: u32,
    y: u32,
    gamma_size: u32,
    mode_valid: u32,
    mode: ModeInfo,
}

/// `struct drm_mode_get_encoder`.
#[repr(C)]
#[derive(Default)]
struct Encoder {
    encoder_id: u32,
    encoder_type: u32,
    crtc_id: u32,
    possible_crtcs: u32,
    possible_clones: u32,
}

/// `struct drm_mode_get_connector`.
#[repr(C)]
#[derive(Default)]
struct Connector {
    encoders_ptr: u64,
    modes_ptr: u64,
    props_ptr: u64,
    prop_values_ptr: u64,
    count_modes: u32,
    count_props: u32,
    count_encoders: u32,
    encoder_id: u32,
    connector_id: u32,
    connector_type: u32,
    connector_type_id: u32,
    connection: u32,
    mm_width: u32,
    mm_height: u32,
    subpixel: u32,
    pad: u32,
}

/// `union drm_wait_vblank`, as its request; the reply overwrites it.
#[repr(C)]
#[derive(Default)]
struct WaitVblank {
    request_type: u32,
    sequence: u32,
    signal_or_reply: [u64; 2],
}

const _: () = {
    assert!(size_of::<WaitVblank>() == 0x18);
    assert!(size_of::<CardResources>() == 0x40);
    assert!(size_of::<ModeInfo>() == 68);
    assert!(size_of::<Crtc>() == 0x68);
    assert!(size_of::<Encoder>() == 0x14);
    assert!(size_of::<Connector>() == 0x50);
};

/// Runs one DRM request on `file`.
fn request<T>(file: &File, code: Ioctl, data: &mut T) -> io::Result<()> {
    // SAFETY: each caller passes the struct that matches `code`, and any arrays its
    // pointers name outlive the call.
    if unsafe { ioctl(file.as_raw_fd(), code, std::ptr::from_mut(data)) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Pixels per line and lines per frame, from the panel's own, that refresh at the
/// rate nearest `fps` with the same pixel clock.
fn timing_for(clock_khz: u32, htotal: u16, vtotal: u16, fps: f64) -> (u16, u16) {
    let rate = |h: u16, v: u16| f64::from(clock_khz) * 1000.0 / (f64::from(h) * f64::from(v));
    let mut best = (htotal, vtotal);
    for h in htotal..=MAX_HTOTAL {
        for v in vtotal..=MAX_VTOTAL {
            if (rate(h, v) - fps).abs() < (rate(best.0, best.1) - fps).abs() {
                best = (h, v);
            }
        }
    }
    best
}

/// The panel's mode changed to `htotal` x `vtotal`, the blank space growing before each
/// sync pulse.
fn stretched(mode: &ModeInfo, (htotal, vtotal): (u16, u16)) -> ModeInfo {
    let (dh, dv) = (htotal - mode.htotal, vtotal - mode.vtotal);
    let hz = f64::from(mode.clock) * 1000.0 / (f64::from(htotal) * f64::from(vtotal));
    let mut changed = ModeInfo {
        hsync_start: mode.hsync_start + dh,
        hsync_end: mode.hsync_end + dh,
        htotal,
        vsync_start: mode.vsync_start + dv,
        vsync_end: mode.vsync_end + dv,
        vtotal,
        vrefresh: hz.round() as u32,
        mode_type: DRM_MODE_TYPE_USERDEF,
        name: [0; 32],
        ..*mode
    };
    let name = format!("{}x{}@{hz:.2}", mode.hdisplay, mode.vdisplay);
    changed.name[..name.len()].copy_from_slice(name.as_bytes());
    changed
}

/// The panel at a game's rate while this is held.
pub struct PanelRate {
    display: File,
    hz: f64,
}

impl PanelRate {
    /// Sets the panel to the rate nearest `fps`, for a game in `MATCHED_FPS`.
    pub fn for_game(fps: f64) -> Option<Self> {
        if !MATCHED_FPS.contains(&fps) {
            return None;
        }
        match Self::set(fps) {
            Ok(rate) => {
                println!("panel: {:.3} Hz for a game at {fps:.3} fps", rate.hz);
                Some(rate)
            }
            Err(error) => {
                eprintln!("warning: the panel stays at {PANEL_HZ} Hz: {error}");
                None
            }
        }
    }

    fn set(fps: f64) -> io::Result<Self> {
        let display = OpenOptions::new().read(true).write(true).open(DISPLAY)?;
        request(&display, DRM_IOCTL_SET_MASTER, &mut ())?;
        let mut resources = CardResources::default();
        request(&display, DRM_IOCTL_MODE_GETRESOURCES, &mut resources)?;
        let mut crtcs = vec![0u32; resources.count_crtcs as usize];
        let mut connectors = vec![0u32; resources.count_connectors as usize];
        let mut encoders = vec![0u32; resources.count_encoders as usize];
        let mut fbs = vec![0u32; resources.count_fbs as usize];
        resources.crtc_id_ptr = crtcs.as_mut_ptr() as u64;
        resources.connector_id_ptr = connectors.as_mut_ptr() as u64;
        resources.encoder_id_ptr = encoders.as_mut_ptr() as u64;
        resources.fb_id_ptr = fbs.as_mut_ptr() as u64;
        request(&display, DRM_IOCTL_MODE_GETRESOURCES, &mut resources)?;
        // The first CRTC is the panel, the one FBIO_WAITFORVSYNC waits on.
        let mut crtc = Crtc { crtc_id: *crtcs.first().ok_or_else(|| io::Error::other("no CRTC"))?, ..Crtc::default() };
        request(&display, DRM_IOCTL_MODE_GETCRTC, &mut crtc)?;
        if crtc.mode_valid == 0 {
            return Err(io::Error::other("the panel is off"));
        }
        let connector = panel_connector(&display, &connectors, crtc.crtc_id)?;
        let timing = timing_for(crtc.mode.clock, crtc.mode.htotal, crtc.mode.vtotal, fps);
        let mode = stretched(&crtc.mode, timing);
        let mut ids = [connector];
        let mut set = Crtc {
            set_connectors_ptr: ids.as_mut_ptr() as u64,
            count_connectors: 1,
            crtc_id: crtc.crtc_id,
            // The framebuffer the CRTC shows now, fbdev's.
            fb_id: u32::MAX,
            mode_valid: 1,
            mode,
            ..Crtc::default()
        };
        request(&display, DRM_IOCTL_MODE_SETCRTC, &mut set)?;
        let hz = f64::from(mode.clock) * 1000.0 / (f64::from(mode.htotal) * f64::from(mode.vtotal));
        Ok(Self { display, hz })
    }

    pub fn hz(&self) -> f64 {
        self.hz
    }

    /// Waits for the panel's next refresh, as `Framebuffer::wait_for_vsync` does without
    /// this. The panel is the first CRTC, which the request names with no CRTC bits.
    pub fn wait_for_vblank(&self) -> io::Result<()> {
        let mut wait = WaitVblank { request_type: DRM_VBLANK_RELATIVE, sequence: 1, ..WaitVblank::default() };
        request(&self.display, DRM_IOCTL_WAIT_VBLANK, &mut wait)
    }
}

/// The connector shown on `crtc_id`.
fn panel_connector(display: &File, connectors: &[u32], crtc_id: u32) -> io::Result<u32> {
    for &id in connectors {
        // One mode's room, so the request doesn't probe the connector again (HDMI's DDC).
        let mut modes = [ModeInfo::default()];
        let mut connector = Connector { connector_id: id, count_modes: 1, modes_ptr: modes.as_mut_ptr() as u64, ..Connector::default() };
        request(display, DRM_IOCTL_MODE_GETCONNECTOR, &mut connector)?;
        if connector.encoder_id == 0 {
            continue;
        }
        let mut encoder = Encoder { encoder_id: connector.encoder_id, ..Encoder::default() };
        request(display, DRM_IOCTL_MODE_GETENCODER, &mut encoder)?;
        if encoder.crtc_id == crtc_id {
            return Ok(id);
        }
    }
    Err(io::Error::other("no connector on the panel's CRTC"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel_mode() -> ModeInfo {
        ModeInfo {
            clock: 24000,
            hdisplay: 640,
            hsync_start: 704,
            hsync_end: 724,
            htotal: 770,
            vdisplay: 480,
            vsync_start: 501,
            vsync_end: 505,
            vtotal: 520,
            ..ModeInfo::default()
        }
    }

    fn rate(clock_khz: u32, (h, v): (u16, u16)) -> f64 {
        f64::from(clock_khz) * 1000.0 / (f64::from(h) * f64::from(v))
    }

    #[test]
    fn european_consoles_get_their_own_rate_within_a_repeat_every_few_minutes() {
        // SNES and NES, Mega Drive, PlayStation.
        for fps in [50.0070, 49.7015, 49.7607, 50.0] {
            let timing = timing_for(24000, 770, 520, fps);
            let hz = rate(24000, timing);
            assert!((hz - fps).abs() < 0.003, "{fps}: {timing:?} at {hz}");
            assert!(timing.0 >= 770 && timing.1 >= 520 && timing.0 <= MAX_HTOTAL && timing.1 <= MAX_VTOTAL);
        }
        assert_eq!(timing_for(24000, 770, 520, 50.0), (800, 600));
    }

    #[test]
    fn the_stretched_mode_keeps_the_picture_and_its_sync_pulses() {
        let mode = stretched(&panel_mode(), (800, 600));
        assert_eq!((mode.hdisplay, mode.vdisplay, mode.clock), (640, 480, 24000));
        assert_eq!((mode.hsync_start, mode.hsync_end, mode.htotal), (734, 754, 800));
        assert_eq!((mode.vsync_start, mode.vsync_end, mode.vtotal), (581, 585, 600));
        assert_eq!(mode.vrefresh, 50);
        assert_eq!(&mode.name[..13], b"640x480@50.00");
    }

    #[test]
    fn only_50_hz_games_change_the_panel() {
        assert!(!MATCHED_FPS.contains(&59.94));
        assert!(!MATCHED_FPS.contains(&60.0988));
        assert!(!MATCHED_FPS.contains(&75.47));
        assert!(MATCHED_FPS.contains(&49.7015));
    }
}
