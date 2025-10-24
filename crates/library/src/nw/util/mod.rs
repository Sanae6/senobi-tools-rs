pub mod res_dict;

use std::{ fmt::Debug};

use snafu::{Backtrace, OptionExt, ResultExt, Snafu};
use zerocopy::{ByteOrder, FromBytes, Immutable, IntoBytes, KnownLayout, U16, U32};

#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct BinaryFileHeader<O: ByteOrder> {
  pub magic: [u8; 8],
  pub micro_version: u8,
  pub minor_version: u8,
  pub major_version: U16<O>,
  pub byte_order_mark: [u8; 2],
  pub packed_alignment: u8,
  pub address_length: u8,
  pub file_name_offset: U32<O>,
  _runtime_relocation_status: U16<O>,
  pub first_block_header: U16<O>,
  pub relocation_table_offset: U32<O>,
  pub file_size: U32<O>,
}

#[derive(Snafu, Debug)]
pub enum BlockError<HandlerError: snafu::Error + snafu::ErrorCompat + 'static> {
  #[snafu(display("block {index}'s header was out of bounds: offset is 0x{offset:X}"))]
  BlockHeaderOutOfBounds {
    offset: usize,
    index: usize,
    backtrace: Backtrace,
  },
  #[snafu(display("block {index}'s data was out of bounds: offset is 0x{offset:X}"))]
  BlockDataOutOfBounds {
    offset: usize,
    index: usize,
    backtrace: Backtrace,
  },
  Block {
    #[snafu(backtrace)]
    source: HandlerError,
  },
}

#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct BinaryBlockHeader<O: ByteOrder> {
  pub magic: [u8; 4],
  pub next_relative_block_offset: U32<O>,
  pub section_size: U32<O>,
  _reserved: U32<O>,
}

pub fn traverse_blocks<O: ByteOrder, HandlerError: snafu::Error + snafu::ErrorCompat>(
  file_data: &[u8],
  first_block_offset: u16,
  // FnMut(magic: [u8; 4], data: &[u8])
  mut block_handler: impl FnMut([u8; 4], &[u8]) -> Result<(), HandlerError>,
) -> Result<(), BlockError<HandlerError>> {
  let mut offset = first_block_offset as usize;
  let mut index = 0usize;

  while offset != 0 {
    let (header, prefix) = BinaryBlockHeader::<O>::read_from_prefix(&file_data[offset..])
      .ok()
      .context(BlockHeaderOutOfBoundsSnafu { offset, index })?;

    let block_data = offset
      .checked_add(header.section_size.get() as usize)
      .and_then(|size| prefix.get(..size))
      .context(BlockDataOutOfBoundsSnafu { index, offset })?;

    block_handler(header.magic, block_data).context(BlockSnafu)?;

    offset = header.next_relative_block_offset.get() as usize;
    index += 1;
  }

  Ok(())
}
