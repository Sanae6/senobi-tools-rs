use std::{
  collections::HashMap,
  ffi::{CStr, FromBytesUntilNulError},
  str::Utf8Error,
};

use snafu::{Backtrace, OptionExt, ResultExt, Snafu, ensure};
use zerocopy::{ByteOrder, FromBytes, Immutable, KnownLayout, U16, U32, U64};

#[derive(Snafu, Debug)]
pub enum ResDictError<ReadError: snafu::Error + snafu::ErrorCompat + 'static> {
  #[snafu(display("the dictionary's header is out of bounds: offset is 0x{offset:X}"))]
  HeaderOutOfBounds { offset: usize, backtrace: Backtrace },
  #[snafu(display("expected magic to be {expected:02X?}, got {actual:02X?}"))]
  IncorrectMagic {
    expected: [u8; 4],
    actual: [u8; 4],
    backtrace: Backtrace,
  },
  #[snafu(display("the dictionary's {count} nodes are out of bounds: offset is 0x{offset:X}"))]
  NodesOutOfBounds {
    offset: usize,
    count: u32,
    backtrace: Backtrace,
  },
  #[snafu(display("the dictionary's {count} values are out of bounds: offset is 0x{offset:X}"))]
  NodeValuesOutOfBounds {
    offset: usize,
    count: u32,
    backtrace: Backtrace,
  },
  #[snafu(display("failed to read node {index}'s value at {offset}"))]
  NodeValueReadFailed {
    index: usize,
    offset: usize,
    #[snafu(backtrace)]
    source: ReadError,
  },
  #[snafu(display("key string {index} is out of bounds: offset is 0x{offset:X}"))]
  KeyStringOutOfBounds {
    index: usize,
    offset: usize,
    backtrace: Backtrace,
  },
  #[snafu(display("key string {index} is unterminated: offset is 0x{offset:X}"))]
  KeyStringUnterminated {
    index: usize,
    offset: usize,
    source: FromBytesUntilNulError,
    backtrace: Backtrace,
  },
  #[snafu(display("key string {index} is out of bounds: offset is 0x{offset:X}"))]
  KeyStringNotUTF8 {
    index: usize,
    offset: usize,
    source: Utf8Error,
    backtrace: Backtrace,
  },
  // apparently keys can just... be the same??
  // #[snafu(display("key string {value} (index: {index}) already exists in the dictionary"))]
  // KeyAlreadyExists {
    // index: usize,
    // value: String,
    // backtrace: Backtrace,
  // },
}

#[derive(FromBytes)]
#[repr(C)]
struct Header<O: ByteOrder> {
  magic: [u8; 4],
  node_count: U32<O>,
  node: Node<O>,
}

#[derive(FromBytes, Immutable)]
#[repr(C)]
struct Node<O: ByteOrder> {
  ref_bit: U32<O>,
  left_node_index: U16<O>,
  right_node_index: U16<O>,
  key_offset: U64<O>,
}

pub fn read_res_dict<
  'a,
  T: FromBytes + Immutable + KnownLayout + 'a,
  E: 'a,
  O: ByteOrder,
  ReadError: snafu::Error + snafu::ErrorCompat + 'static,
>(
  file_data: &'a [u8],
  dict_offset: usize,
  values_offset: usize,
  // FnMut(key: &str, element: &mut T)
  mut node_validator: impl FnMut(&'a str, &'a T) -> Result<E, ReadError>,
) -> Result<Vec<(&'a str, E)>, ResDictError<ReadError>> {
  let header_end_offset =
    dict_offset
      .checked_add(size_of::<Header<O>>())
      .context(HeaderOutOfBoundsSnafu {
        offset: dict_offset,
      })?;

  let header = file_data
    .get(dict_offset..header_end_offset)
    .context(HeaderOutOfBoundsSnafu {
      offset: dict_offset,
    })?;
  let header =
    Header::<O>::read_from_bytes(header).expect("failed to validate header slice's size");

  ensure!(
    header.magic == *b"_DIC",
    IncorrectMagicSnafu {
      expected: *b"_DIC",
      actual: header.magic
    }
  );

  let node_count = header.node_count.get();

  let nodes_end_offset = size_of::<Node<O>>()
    .checked_mul(node_count as _)
    .and_then(|size| header_end_offset.checked_add(size))
    .context(NodesOutOfBoundsSnafu {
      offset: header_end_offset,
      count: node_count,
    })?;

  let nodes =
    file_data
      .get(header_end_offset..nodes_end_offset)
      .context(NodesOutOfBoundsSnafu {
        offset: header_end_offset,
        count: node_count,
      })?;

  let nodes = <[Node<O>]>::ref_from_bytes_with_elems(nodes, node_count as _)
    .expect("failed to validate nodes slice's size");

  let values_end_offset = size_of::<T>()
    .checked_mul(node_count as _)
    .and_then(|size| values_offset.checked_add(size))
    .context(NodeValuesOutOfBoundsSnafu {
      offset: values_offset,
      count: node_count,
    })?;

  let values =
    file_data
      .get(values_offset..values_end_offset)
      .context(NodeValuesOutOfBoundsSnafu {
        offset: values_offset,
        count: node_count,
      })?;

  let values = values.chunks_exact(size_of::<T>());

  let mut dictionary = Vec::new();

  for (index, (node, value_data)) in nodes.iter().zip(values).enumerate() {
    let key_offset = node.key_offset.get() as usize + size_of::<u16>();

    let key = file_data
      .get(key_offset..)
      .context(KeyStringOutOfBoundsSnafu {
        offset: key_offset,
        index,
      })?;

    let key = CStr::from_bytes_until_nul(key).context(KeyStringUnterminatedSnafu {
      offset: key_offset,
      index,
    })?;

    let key = key.to_str().context(KeyStringNotUTF8Snafu {
      offset: key_offset,
      index,
    })?;

    let value = T::ref_from_bytes(value_data).unwrap();
    let value = node_validator(key, value).context(NodeValueReadFailedSnafu {
      index,
      offset: values_offset + size_of::<T>() * index,
    })?;

    dictionary.push((key, value));
  }

  Ok(dictionary)
}
