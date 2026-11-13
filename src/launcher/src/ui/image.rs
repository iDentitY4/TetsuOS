//! A picture in memory: a cover, a badge or a screenshot.

use crate::ui::color::Rgb;

pub struct Image {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<Rgb>,
}
