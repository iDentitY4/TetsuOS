mod canvas;
mod fb;
mod font;

use std::io::{self, Write};
use std::process::ExitCode;
use std::thread::sleep;
use std::time::Duration;

const FB_PATH: &str = "/dev/fb0";
/// The display drivers may finish probing after init starts.
const FB_WAIT_SECONDS: u32 = 10;
const PREVIEW_SIZE: (usize, usize) = (640, 480);

fn kernel_release() -> String {
    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|release| release.trim().to_string())
        .unwrap_or_else(|_| "UNKNOWN".to_string())
}

fn open_framebuffer() -> io::Result<fb::Framebuffer> {
    let mut remaining = FB_WAIT_SECONDS;
    loop {
        match fb::Framebuffer::open(FB_PATH) {
            Err(error) if error.kind() == io::ErrorKind::NotFound && remaining > 0 => {
                remaining -= 1;
                sleep(Duration::from_secs(1));
            }
            result => return result,
        }
    }
}

fn run_on_device() -> io::Result<()> {
    let mut framebuffer = open_framebuffer()?;
    // Non-fatal: at worst, console text could appear over the screen.
    let _console = fb::claim_console()
        .inspect_err(|error| eprintln!("warning: could not claim console: {error}"));

    let (width, height) = framebuffer.size();
    framebuffer.draw(&canvas::welcome_screen(width, height, &kernel_release()))?;
    println!("drew {width}x{height} welcome screen");

    loop {
        sleep(Duration::from_secs(3600));
    }
}

/// Renders to a PPM image, to check the layout without the device.
fn write_preview(path: &str) -> io::Result<()> {
    let (width, height) = PREVIEW_SIZE;
    let canvas = canvas::welcome_screen(width, height, &kernel_release());
    let mut file = io::BufWriter::new(std::fs::File::create(path)?);
    write!(file, "P6\n{width} {height}\n255\n")?;
    for pixel in &canvas.pixels {
        file.write_all(&[pixel.0, pixel.1, pixel.2])?;
    }
    file.flush()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [] => run_on_device(),
        [flag, path] if flag == "--preview" => write_preview(path),
        _ => {
            eprintln!("usage: welcome [--preview FILE.ppm]");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("welcome: {error}");
            ExitCode::FAILURE
        }
    }
}
