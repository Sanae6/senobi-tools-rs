use modular_bitfield::prelude::*;

pub mod bc1;

pub struct Srgb;
pub struct SignedNorm;
pub struct UnsignedNorm;

#[bitfield]
pub struct Rgb565 {
  r: B5,
  g: B6,
  b: B5,
}
