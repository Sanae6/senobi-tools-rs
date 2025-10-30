use std::{collections::HashMap, ffi::CStr};

use snafu::{Backtrace, OptionExt, ResultExt, Snafu, ensure};
use tegra_swizzle::{surface::deswizzle_surface, SwizzleError};
use zerocopy::{ByteOrder, FromBytes, I32, Immutable, IntoBytes, KnownLayout, U32, U64};

use crate::nw::{
  gfx::{decode_image_format, ChannelFormat, FormatInfo, TextureInfo, TypeFormat},
  util::{
    res_dict::{read_res_dict, ResDictError}, BinaryBlockHeader, BinaryFileHeader
  },
};

#[derive(Snafu, Debug)]
pub enum BntxError {
  #[snafu(display("the header is out of bounds"))]
  HeaderOutOfBounds { backtrace: Backtrace },
  #[snafu(display("expected magic to be {expected:02X?}, got {actual:02X?}"))]
  IncorrectMagic {
    expected: [u8; 8],
    actual: [u8; 8],
    backtrace: Backtrace,
  },
  #[snafu(display("the texture container header is out of bounds"))]
  ResTextureContainerHeaderOutOfBounds { backtrace: Backtrace },
  #[snafu(display("failed to read texture"))]
  TextureInfo {
    #[snafu(backtrace)]
    source: Box<ResDictError<BntxError>>,
  },
  TextureInfoOutOfBounds {
    key: String,
    offset: usize,
    backtrace: Backtrace,
  },
  InvalidImageFormat {
    key: String,
    actual: u32,
    backtrace: Backtrace,
  },
  MipmapPointersOutOfBounds {
    key: String,
    offset: usize,
    levels: u16,
    backtrace: Backtrace,
  },
  #[snafu(display("texture {key:?}'s mipmap {level} is out of bounds: offset is 0x{offset:X}"))]
  MipmapOutOfBounds {
    key: String,
    level: u16,
    offset: usize,
    backtrace: Backtrace,
  },
}
#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct ResTextureContainer<O: ByteOrder> {
  magic: [u8; 4],
  texture_count: U32<O>,
  texture_info_values_offset: U64<O>,
  gpu_region_header_offset: U64<O>,
  texture_info_dictionary_offset: U64<O>,
  runtime_memory_pool_region: U64<O>,
  runtime_memory_pool_ptr: U64<O>,
  memory_pool_offset: I32<O>,
  reserved: U32<O>,
}

#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct ResTextureInfo<O: ByteOrder> {
  block_header: BinaryBlockHeader<O>,
  pub info: TextureInfo<O>,
  pub packaged_texture_layout: [u8; 4],
  reserved_1: [u8; 0x14],
  pub total_texture_size: U32<O>,
  pub texture_data_alignment: U32<O>,
  pub channel_sources: [u8; 4],
  pub image_dimension: u8,
  reserved_2: [u8; 3],
  pub texture_name: U64<O>,
  pub parent_texture_container: U64<O>,
  pub mipmap_array: U64<O>,
  pub user_data_array: U64<O>,
  pub runtime_texture: U64<O>,
  pub runtime_texture_view: U64<O>,
  pub runtime_descriptor_slot: U64<O>,
  pub user_data_dictionary: U64<O>,
}

pub struct BntxReader<'a, O: ByteOrder + 'static> {
  file_data: &'a [u8],
  pub textures: HashMap<&'a str, BntxTextureReader<'a, O>>,
}

impl<'a, O: ByteOrder> BntxReader<'a, O> {
  pub fn read(file_data: &'a [u8]) -> Result<Self, BntxError> {
    let header_offset_end = size_of::<BinaryFileHeader<O>>();
    let header = file_data
      .get(..header_offset_end)
      .map(|data| BinaryFileHeader::<O>::ref_from_bytes(data).unwrap())
      .context(HeaderOutOfBoundsSnafu)?;

    ensure!(
      header.magic == *b"BNTX\0\0\0\0",
      IncorrectMagicSnafu {
        expected: *b"BNTX\0\0\0\0",
        actual: header.magic
      }
    );

    let container_offset_end = header_offset_end + size_of::<ResTextureContainer<O>>();
    let container = file_data
      .get(header_offset_end..container_offset_end)
      .map(|data| ResTextureContainer::<O>::read_from_bytes(data).unwrap())
      .context(ResTextureContainerHeaderOutOfBoundsSnafu)?;

    println!(
      "name: {:?}",
      CStr::from_bytes_until_nul(&file_data[header.file_name_offset.get() as usize..])
    );

    let textures = read_res_dict::<U64<O>, BntxTextureReader<'a, O>, O, BntxError>(
      file_data,
      container.texture_info_dictionary_offset.get() as _,
      container.texture_info_values_offset.get() as _,
      |key, texture| {
        let offset = texture.get() as usize;
        let info = offset
          .checked_add(size_of::<ResTextureInfo<O>>())
          .and_then(|end_offset| file_data.get(offset..end_offset))
          .map(|data| ResTextureInfo::<O>::ref_from_bytes(data).unwrap())
          .context(TextureInfoOutOfBoundsSnafu {
            offset,
            key: key.to_owned(),
          })?;
        println!("{key}");

        let decoded = decode_image_format(info.info.image_format.get()).expect("fuck");
        println!("{decoded:?}");
        let array_layer_count = info.info.array_layers.get();
        let mip_level_count = info.info.mip_levels.get() as usize;

        let mipmap_ptrs_offset = info.mipmap_array.get() as usize;
        let mipmap_ptr_array = mipmap_ptrs_offset
          .checked_add(size_of::<u64>() * mip_level_count)
          .and_then(|end_offset| file_data.get(mipmap_ptrs_offset..end_offset))
          .map(|data| <[U64<O>]>::ref_from_bytes_with_elems(data, mip_level_count).unwrap())
          .context(MipmapPointersOutOfBoundsSnafu {
            offset: mipmap_ptrs_offset,
            key: key.to_owned(),
            levels: mip_level_count as u16,
          })?;

        let mut array_layers = Vec::with_capacity(array_layer_count as usize);
        let mut array_offset = 0;
        for array_layer in 0..array_layer_count {
          let mut next_array_offset = 0;
          let mut mipmaps = Vec::with_capacity(array_layer_count as usize);
          for mip_level in 0..mip_level_count {
            let start_offset = mipmap_ptr_array[0].get() as usize;
            let index = (array_layer as usize * mip_level_count) + mip_level;
            let offset = mipmap_ptr_array[index].get() as usize;
            // .checked_mul(array_offset)
            // .context(MipmapOutOfBoundsSnafu {
            //   key: key.to_owned(),
            //   level: mip_level as u16,
            //   offset: mipmap_ptr_array[index].get() as usize,
            // })?;

            println!("{index} {} {}", info.info.width, info.info.height);
            println!(
              "mip_offset {offset} {array_offset} {start_offset} {}",
              info.total_texture_size.get()
            );
            let size = start_offset + info.total_texture_size.get() as usize - offset;
            println!("mip_size {size} {}", offset + size);

            let mipmap = start_offset
              .checked_add(info.total_texture_size.get() as _)
              .and_then(|size| size.checked_sub(offset))
              .and_then(|size| size.checked_div(array_layer_count as usize))
              .and_then(|size| file_data.get(offset..offset + size))
              .context(MipmapOutOfBoundsSnafu {
                key: key.to_owned(),
                level: mip_level as u16,
                offset,
              })?;

            if mip_level == 0 {
              next_array_offset += mipmap.len();
            }

            mipmaps.push(mipmap);
          }

          // panic!();

          array_layers.push(mipmaps);
          array_offset += next_array_offset;
        }
        println!("henlo {key:?}");

        Ok(BntxTextureReader {
          file_data,
          info,
          array_levels: array_layers,
        })
      },
    )
    .map_err(Box::new)
    .context(TextureInfoSnafu)?;

    Ok(Self {
      file_data,
      textures,
    })
  }
}

pub struct BntxTextureReader<'a, O: ByteOrder + 'static> {
  file_data: &'a [u8],
  array_levels: Vec<Vec<&'a [u8]>>,
  pub info: &'a ResTextureInfo<O>,
}

impl<'a, O: ByteOrder> BntxTextureReader<'a, O> {
  pub fn width(&self) -> u32 {
    self.info.info.width.get()
  }
  pub fn height(&self) -> u32 {
    self.info.info.height.get()
  }
  pub fn depth(&self) -> u32 {
    self.info.info.depth.get()
  }
  pub fn array_layers(&self) -> u32 {
    self.info.info.array_layers.get()
  }
  pub fn mip_levels(&self) -> u32 {
    self.info.info.mip_levels.get() as u32
  }

  pub fn image_format(&self) -> (ChannelFormat, TypeFormat) {
    decode_image_format(self.info.info.image_format.get()).unwrap()
  }

  pub fn image_data(&self) -> &'a [u8] {
    let start_offset = self.info.mipmap_array.get() as usize;
    let data_start_ptr = U64::<O>::read_from_bytes(
      self
        .file_data
        .get(start_offset..start_offset + size_of::<u64>())
        .unwrap(),
    )
    .unwrap()
    .get() as usize;
    let data_end_ptr = data_start_ptr + self.info.total_texture_size.get() as usize;

    &self.file_data[data_start_ptr..data_end_ptr]
  }

  pub fn deswizzled_image_data(&self) -> Result<Vec<u8>, SwizzleError> {
    let (chan_fmt, type_fmt) = self.image_format();
    let format_info = FormatInfo::from_image_format(chan_fmt, type_fmt).unwrap();
    deswizzle_surface(
      self.width(),
      self.height(),
      self.depth(),
      self.image_data(),
      format_info.block_dim,
      None,
      format_info.bytes_per_pixel,
      self.mip_levels(),
      self.array_layers(),
    )
  }
}
