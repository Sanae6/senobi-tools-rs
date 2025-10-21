use std::{backtrace::Backtrace, marker::PhantomData, ops::Range};

use snafu::{Snafu, ensure};
use zerocopy::{ByteOrder, FromBytes};

use crate::sarc::types::{SarcHeader, SfatHeader, SfatNode, SfntHeader};

#[derive(Snafu, Debug)]
pub enum ReadError {
  #[snafu(display("the file header is out of bounds"))]
  HeaderOutOfBounds {
    backtrace: Backtrace,
  },
  IncorrectHeaderMagic {
    expected: [u8; 4],
    actual: [u8; 4],
    backtrace: Backtrace,
  },
  InvalidByteOrderMark {
    actual: [u8; 2],
    backtrace: Backtrace,
  },
  UnsupportedVersion {
    actual: u16,
    backtrace: Backtrace,
  },
  IncorrectHeaderLength {
    expected: u16,
    actual: u16,
    backtrace: Backtrace,
  },
  #[snafu(display(
    "the file data is out of bounds, starts at 0x{file_start:08X}, size is 0x{file_size:08X}"
  ))]
  FileDataOutOfBounds {
    file_start: u32,
    file_size: u32,
    backtrace: Backtrace,
  },
  #[snafu(display("the node table header is out of bounds"))]
  NodeTableHeaderOutOfBounds {
    backtrace: Backtrace,
  },
  IncorrectNodeTableHeaderMagic {
    expected: [u8; 4],
    actual: [u8; 4],
    backtrace: Backtrace,
  },
  IncorrectNodeTableHeaderLength {
    expected: u16,
    actual: u16,
    backtrace: Backtrace,
  },
  NodeOutOfBounds {
    range: Range<u32>,
    backtrace: Backtrace,
  },
  NodeDataOutOfBounds {
    range: Range<u32>,
    backtrace: Backtrace,
  },
  #[snafu(display("the node table header is out of bounds, tried to fetch {range:08X?}"))]
  NameTableHeaderOutOfBounds {
    range: Range<u32>,
    backtrace: Backtrace,
  },
  IncorrectNameTableHeaderMagic {
    expected: [u8; 4],
    actual: [u8; 4],
    backtrace: Backtrace,
  },
  IncorrectNameTableHeaderLength {
    expected: u16,
    actual: u16,
    backtrace: Backtrace,
  },
  NameOutOfBounds {
    offset: u32,
    range: Range<u32>,
    backtrace: Backtrace,
  },
}

pub struct SarcReader<'a, O: ByteOrder> {
  file_data: &'a [u8],
  phantom: PhantomData<O>,
}

impl<'a, O: ByteOrder> SarcReader<'a, O> {
  pub fn new(data: &[u8]) -> Result<Self, ReadError> {
    ensure!(
      data.len() > size_of::<SarcHeader<O>>(),
      HeaderOutOfBoundsSnafu
    );
    let sarc_header = SarcHeader::<O>::ref_from_bytes(&data[..size_of::<SarcHeader<O>>()]).unwrap();
    ensure!(
      sarc_header.magic == *b"SARC",
      IncorrectHeaderMagicSnafu {
        expected: *b"SARC",
        actual: sarc_header.magic
      }
    );

    ensure!(
      (sarc_header.data_start.get() as usize) < data.len()
        && sarc_header
          .data_start
          .get()
          .checked_add(sarc_header.file_size.get())
          .is_some(),
      FileDataOutOfBoundsSnafu {
        file_start: sarc_header.data_start.get() as _,
        file_size: sarc_header.file_size.get() as _
      }
    );

    let mut offset = size_of::<SarcHeader<O>>();
    ensure!(
      data.len() > offset + size_of::<SfatHeader<O>>(),
      NodeTableHeaderOutOfBoundsSnafu
    );
    let sfat_header =
      SfatHeader::<O>::ref_from_bytes(&data[offset..offset + size_of::<SfatHeader<O>>()]).unwrap();
    ensure!(
      sfat_header.magic == *b"SFAT",
      IncorrectNodeTableHeaderMagicSnafu {
        expected: *b"SFAT",
        actual: sfat_header.magic
      }
    );
    ensure!(
      sfat_header.header_length.get() == 0xC,
      IncorrectNodeTableHeaderLengthSnafu {
        expected: 0xCu16,
        actual: sfat_header.header_length.get()
      }
    );
    offset += size_of::<SfatHeader<O>>();

    let node_count = sfat_header.node_count.get() as usize;
    ensure!(
      data.len() > offset + size_of::<SfatNode::<O>>() * node_count as usize,
      NodeOutOfBoundsSnafu {
        range: offset as u32..(offset + size_of::<SfatNode::<O>>()) as u32 * node_count as u32
      }
    );
    let sfat_nodes = <[SfatNode<O>]>::ref_from_bytes_with_elems(
      &data[offset..offset as usize + size_of::<SfatNode<O>>() * node_count as usize],
      node_count,
    )
    .unwrap();
    offset += size_of::<SfatNode<O>>() * node_count as usize;

    ensure!(
      data.len() > offset + size_of::<SfntHeader<O>>(),
      NameTableHeaderOutOfBoundsSnafu {
        range: offset as u32..(offset + size_of::<SfntHeader<O>>()) as u32
      }
    );
    let sfnt_header =
      SfntHeader::<O>::ref_from_bytes(&data[offset..offset + size_of::<SfntHeader<O>>()]).unwrap();
    ensure!(
      sfnt_header.magic == *b"SFNT",
      IncorrectNodeTableHeaderMagicSnafu {
        expected: *b"SFNT",
        actual: sfnt_header.magic
      }
    );
    ensure!(
      sfnt_header.header_length.get() == 0x8,
      IncorrectNameTableHeaderLengthSnafu {
        expected: 0x8u16,
        actual: sfnt_header.header_length.get()
      }
    );
    offset += size_of::<SfntHeader<O>>();

    for node in sfat_nodes {
      if node.file_attributes.get() & 0x01000000 != 0 {
        let name_offset = (node.file_attributes & 0xFFFF) * 4;
        // ensure!()
      }
    }

    todo!()
  }
}
