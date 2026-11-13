//! A colour, as every layer from the settings to the framebuffer passes it on.

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// This colour moved `percent` of the way to `other`.
    pub fn toward(self, other: Rgb, percent: u32) -> Rgb {
        let mix = |from: u8, to: u8| ((u32::from(from) * (100 - percent) + u32::from(to) * percent) / 100) as u8;
        Rgb(mix(self.0, other.0), mix(self.1, other.1), mix(self.2, other.2))
    }
}
