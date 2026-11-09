//! A colour, as every layer from the settings to the framebuffer passes it on.

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);
