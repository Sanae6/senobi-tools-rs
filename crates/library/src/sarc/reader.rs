use std::{backtrace::Backtrace, ffi::CStr, marker::PhantomData, ops::Range};

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
    relative_start: u32,
    relative_end: u32,
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
  #[snafu(display("node name was out of bounds at offset {offset:08X?}"))]
  NameOutOfBounds {
    offset: u32,
    backtrace: Backtrace,
  },
  #[snafu(display("node name was unterminated at offset {offset:08X?}"))]
  NameUnterminated {
    offset: u32,
    backtrace: Backtrace,
  },
}

pub struct SarcReader<'a, O: ByteOrder> {
  file_data: &'a [u8],
  name_data: &'a [u8],
  nodes: &'a [SfatNode<O>],
  phantom: PhantomData<O>,
}

impl<'a, O: ByteOrder> SarcReader<'a, O> {
  pub fn new(data: &'a [u8]) -> Result<Self, ReadError> {
    assert!(
      size_of::<usize>() >= 4,
      "cannot be executed on 16 bit platforms"
    );
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
        file_start: sarc_header.data_start.get(),
        file_size: sarc_header.file_size.get()
      }
    );

    let file_data = &data[sarc_header.data_start.get() as usize..];

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
    let nodes = <[SfatNode<O>]>::ref_from_bytes_with_elems(
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
    let name_data = &data[offset..];

    for node in nodes {
      if let Some(name_offset) = node.name_offset() {
        let name_start = offset + name_offset as usize;
        ensure!(
          name_start < data.len(),
          NameOutOfBoundsSnafu {
            offset: name_offset
          }
        );

        let cstr = CStr::from_bytes_until_nul(&data[name_start..]);
        ensure!(
          cstr.is_ok(),
          NameOutOfBoundsSnafu {
            offset: name_offset
          }
        )
      }

      ensure!(
        (node.relative_file_start.get() as usize) <= file_data.len()
          && (node.relative_file_end.get() as usize) <= file_data.len(),
        NodeDataOutOfBoundsSnafu {
          relative_start: node.relative_file_start.get(),
          relative_end: node.relative_file_end.get()
        }
      );
    }

    Ok(Self {
      file_data,
      name_data,
      nodes,
      phantom: PhantomData,
    })
  }

  pub fn get(&self, search_name: &str) -> Option<&'a [u8]> {
    self.entries().find_map(|(name, data)| {
      name.and_then(|name| name.to_bytes().eq(search_name.as_bytes()).then_some(data))
    })
  }

  pub fn entries(&self) -> impl Iterator<Item = (Option<&'a CStr>, &'a [u8])> {
    self.nodes.iter().map(|node| {
      (
        node.name_offset().map(|name_offset| {
          CStr::from_bytes_until_nul(&self.name_data[name_offset as usize..])
            .expect("poorly asserted name during parsing and validation")
        }),
        &self.file_data
          [node.relative_file_start.get() as usize..node.relative_file_end.get() as usize],
      )
    })
  }
}
