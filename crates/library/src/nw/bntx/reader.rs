use std::ffi::CStr;

use snafu::{Backtrace, OptionExt, ResultExt, Snafu, ensure};
use zerocopy::{ByteOrder, FromBytes, I32, Immutable, IntoBytes, KnownLayout, U32, U64};

use crate::nw::{
  gfx::{decode_image_format, TextureInfo},
  util::{
    res_dict::{read_res_dict, ResDictError}, BinaryBlockHeader, BinaryFileHeader
  },
};

#[derive(Snafu, Debug)]
pub enum BntxError {
  #[snafu(display("the header is out of bounds"))]
  HeaderOutOfBounds {
    backtrace: Backtrace,
  },
  #[snafu(display("expected magic to be {expected:02X?}, got {actual:02X?}"))]
  IncorrectMagic {
    expected: [u8; 8],
    actual: [u8; 8],
    backtrace: Backtrace,
  },
  #[snafu(display("the texture container header is out of bounds"))]
  ResTextureContainerHeaderOutOfBounds {
    backtrace: Backtrace,
  },
  #[snafu(display("failed to read texture"))]
  TextureInfo {
    #[snafu(backtrace)]
    source: Box<ResDictError<BntxError>>,
  },
  TextureInfoOutOfBounds {
    key: String,
    offset: usize,
  },
  MipmapPointersOutOfBounds {
    key: String,
    offset: usize,
    levels: u16,
  },
  MipmapOutOfBounds {
    key: String,
    level: u16,
    offset: usize,
  }
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

pub struct Bntx<'a, O: ByteOrder + 'static> {
  file_data: &'a [u8],
  pub textures: Vec<(&'a str, &'a ResTextureInfo<O>)>,
}

impl<'a, O: ByteOrder> Bntx<'a, O> {
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

    let textures = read_res_dict::<U64<O>, &ResTextureInfo<O>, O, BntxError>(
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
        let mip_levels = info.info.mip_levels.get() as usize;

        let mipmap_ptrs_offset = info.mipmap_array.get() as usize;
        let mipmap_ptr_array = mipmap_ptrs_offset
          .checked_add(size_of::<u64>() * mip_levels)
          .and_then(|end_offset| file_data.get(mipmap_ptrs_offset..end_offset))
          .map(|data| <[U64<O>]>::ref_from_bytes_with_elems(data, mip_levels).unwrap())
          .context(MipmapPointersOutOfBoundsSnafu {
            offset: mipmap_ptrs_offset,
            key: key.to_owned(),
            levels: mip_levels as u16
          })?;
        // for mip_level in 0..mip_levels {
          // let offset = mipmap_ptr_array[mip_level as usize].get();
          // let mipmap_array = offset
          // .checked_add(size_of::<u64>())
          // .and_then(|end_offset| file_data.get(mipmap_ptrs_offset..end_offset));
        // }
        println!("henlo {key:?}");

        Ok(info)
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

struct Texture<'a, O: ByteOrder> {
  file_data: &'a [u8],
  pub info: &'a ResTextureInfo<O>,
}

impl<'a, O: ByteOrder> Texture<'a, O> {
  pub fn mipmaps(&self) -> impl Iterator<Item = &[u8]> {
    let mip_levels = self.info.info.mip_levels.get();

    (0..mip_levels).map(|_| self.file_data)
  }
}
