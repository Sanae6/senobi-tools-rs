use modular_bitfield::prelude::*;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use zerocopy::{ByteOrder, FromBytes, Immutable, IntoBytes, KnownLayout, U16, U32, Unaligned};

#[bitfield(bytes = 1)]
#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
#[repr(transparent)]
pub struct TextureInfoFlags {
  pub packaged_texture: bool,
  pub sparse_binding: bool,
  pub sparse: bool,
  pub res_texture: bool,
  padding: B4,
}

#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct TextureInfo<O: ByteOrder> {
  pub flags: TextureInfoFlags,
  pub storage_dimension: u8,
  pub tile_mode: U16<O>,
  // what is swizzle??
  pub swizzle: U16<O>,
  pub mip_levels: U16<O>,
  pub sample_count: U16<O>,
  _reserved: U16<O>,
  pub image_format: U32<O>,
  pub access_flags: U32<O>,
  pub width: U32<O>,
  pub height: U32<O>,
  pub depth: U32<O>,
  pub array_layers: U32<O>,
  // what does this mean???
  pub packaged_texture_layout: U32<O>,
}

#[derive(Debug, FromPrimitive)]
pub enum ChannelFormat {
  None = 0x1,
  R8 = 0x2,
  R4G4B4A4 = 0x3,
  R5G5B5A1 = 0x5,
  A1B5G5R5 = 0x6,
  R5G6B5 = 0x7,
  B5G6R5 = 0x8,
  R8G8 = 0x9,
  R16 = 0xa,
  R8G8B8A8 = 0xb,
  B8G8R8A8 = 0xc,
  R9G9B9E5F = 0xd,
  R10G10B10A2 = 0xe,
  R11G11B10F = 0xf,
  R16G16 = 0x12,
  D24S8 = 0x13,
  R32 = 0x14,
  R16G16B16A16 = 0x15,
  D32FS8 = 0x16,
  R32G32 = 0x17,
  R32G32B32 = 0x18,
  R32G32B32A32 = 0x19,
  BC1 = 0x1a,
  BC2 = 0x1b,
  BC3 = 0x1c,
  BC4 = 0x1d,
  BC5 = 0x1e,
  BC6H = 0x1f,
  BC7U = 0x20,
  ASTC_4x4 = 0x2d,
  ASTC_5x4 = 0x2e,
  ASTC_5x5 = 0x2f,
  ASTC_6x5 = 0x30,
  ASTC_6x6 = 0x31,
  ASTC_8x5 = 0x32,
  ASTC_8x6 = 0x33,
  ASTC_8x8 = 0x34,
  ASTC_10x5 = 0x35,
  ASTC_10x6 = 0x26,
  ASTC_10x8 = 0x37,
  ASTC_10x10 = 0x38,
  ASTC_12x10 = 0x39,
  ASTC_12x12 = 0x3a,
  B5G5R5A1 = 0x3b,
}

#[derive(Debug, FromPrimitive)]
pub enum TypeFormat {
  Unorm = 0x1,
  Snorm = 0x2,
  UInt = 0x3,
  SInt = 0x4,
  Float = 0x5,
  SRGB = 0x6,
  Depth = 0x7,
  UScaled = 0x8,
  SScaled = 0x9,
  UFloat = 0xa,
}

pub fn decode_image_format(value: u32) -> Option<(ChannelFormat, TypeFormat)> {
  ChannelFormat::from_u32((value & 0xff00) >> 8).zip(TypeFormat::from_u32(value & 0xff))
}
