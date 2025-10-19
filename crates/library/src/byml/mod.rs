pub mod reader;
mod types;
pub mod writer;

pub const MAXIMUM_SUPPORTED_VERSION: u16 = 3;

#[derive(Debug)]
pub enum Order {
  LittleEndian,
  BigEndian,
}

use string_table_error::StringTableError;
mod string_table_error {
  use std::backtrace::Backtrace;

  use snafu::Snafu;

  #[derive(Snafu, Debug)]
  pub enum StringTableError {
    #[snafu(display("string table node header was out of bounds"))]
    HeaderOutOfBounds {
      size: usize,
      offset: u32,
      backtrace: Backtrace,
    },
    #[snafu(display("string table address table was out of bounds"))]
    AddressTableOutOfBounds {
      size: usize,
      offset: u32,
      backtrace: Backtrace,
    },
    #[snafu(display("string table strings were out of bounds"))]
    StringDataEndOutOfBounds {
      size: usize,
      offset: u32,
      backtrace: Backtrace,
    },
  }
}

mod array_error {
  use snafu::Snafu;

  #[derive(Snafu, Debug)]
  pub enum ContainerError {
    #[snafu(display("not an array, got data type {value:02X}"))]
    IncorrectDataType {
      value: u8,
      backtrace: snafu::Backtrace,
    },
    #[snafu(display("array data type list was out of bounds"))]
    DataTypesOutOfBounds {
      size: usize,
      offset: u32,
      backtrace: snafu::Backtrace,
    },
    #[snafu(display("not an array, got data type {value:02X}"))]
    InvalidElementDataType {
      element_index: usize,
      value: u8,
      backtrace: snafu::Backtrace,
    },
    #[snafu(display("array value list was out of bounds"))]
    ValuesOutOfBounds {
      size: usize,
      offset: u32,
      backtrace: snafu::Backtrace,
    },
    #[snafu(display("no hash key table was available while reading dictionary"))]
    NoHashKeyTable { backtrace: snafu::Backtrace },
  }
}

use open_error::OpenError;
mod open_error {
  use std::backtrace::Backtrace;

  use snafu::Snafu;

  use crate::byml::{Order, StringTableError, array_error::ContainerError, types::DataType};

  #[derive(Snafu, Debug)]
  pub enum OpenError {
    #[snafu(display("expected byml endianness to be {expected:?}, got {actual:?}"))]
    EndiannessMismatch {
      expected: Order,
      actual: Order,
      backtrace: Backtrace,
    },
    #[snafu(display("unsupported version {actual}, greatest supported version is {maximum}"))]
    UnsupportedVersion {
      maximum: u16,
      actual: u16,
      backtrace: Backtrace,
    },
    #[snafu(display("attempted to read out of bounds: size is {size} but offset is {offset}"))]
    NotEnoughDataForHeader {
      size: usize,
      offset: u32,
      backtrace: Backtrace,
    },
    #[snafu(display("root node points out of bounds"))]
    RootNodeOutOfBounds {
      size: usize,
      offset: u32,
      backtrace: Backtrace,
    },
    #[snafu(display("root node pointer is misaligned"))]
    RootNodeMisaligned {
      size: usize,
      offset: u32,
      backtrace: Backtrace,
    },
    #[snafu(display("string table points out of bounds"))]
    StringTableOutOfBounds {
      size: usize,
      offset: u32,
      backtrace: Backtrace,
    },
    #[snafu(display("string table pointer is misaligned"))]
    StringTableMisaligned {
      size: usize,
      offset: u32,
      backtrace: Backtrace,
    },
    #[snafu(display("string table could not be deserialized: {source}"))]
    StringTable {
      #[snafu(backtrace)]
      source: StringTableError,
    },
    #[snafu(display("hash key table points out of bounds"))]
    HashKeyTableOutOfBounds {
      size: usize,
      offset: u32,
      backtrace: Backtrace,
    },
    #[snafu(display("string table pointer is misaligned"))]
    HashKeyTableMisaligned {
      size: usize,
      offset: u32,
      backtrace: Backtrace,
    },
    #[snafu(display("hash key table table could not be deserialized: {source}"))]
    HashKeyTable {
      #[snafu(backtrace)]
      source: StringTableError,
    },
    #[snafu(display("root node was not a valid data type: data type is {value:02X}"))]
    InvalidDataType { value: u8, backtrace: Backtrace },
    #[snafu(display("root type was not a container type: data type is {value:?}"))]
    NonContainerType {
      value: DataType,
      backtrace: Backtrace,
    },
    #[snafu(display("error while deserializing array: {source}"))]
    Container {
      #[snafu(backtrace)]
      source: ContainerError,
    },
  }
}

use string_read_error::StringReadError;
mod string_read_error {
  use std::str::Utf8Error;

  use snafu::Snafu;

  #[derive(Snafu, Debug)]
  pub enum StringReadError {
    OffsetEntryOutOfBounds {
      offset: u32,
    },
    OffsetOutsideOfStringData,
    UnterminatedString,
    #[snafu(display("{error}"))]
    NonUtf8String {
      error: Utf8Error,
    },
  }
}

use element_error::ElementReadError;
mod element_error {
  use std::str::Utf8Error;

  use snafu::Snafu;

  use crate::byml::{StringReadError, array_error::ContainerError, types::DataType};

  #[derive(Snafu, Debug)]
  pub enum ElementReadError {
    #[snafu(display("failed to retrieve string: {source}"))]
    StringReadError {
      source: StringReadError,
      backtrace: snafu::Backtrace,
    },
    #[snafu(display("no string table when string was requested"))]
    NoStringTable { backtrace: snafu::Backtrace },
    #[snafu(display("no hash key table when dictionary was requested"))]
    NoHashKeyTable { backtrace: snafu::Backtrace },
    #[snafu(display("value is out of bounds"))]
    ValueOutOfBounds {
      size: usize,
      offset: u32,
      backtrace: snafu::Backtrace,
    },
    #[snafu(display("pointer for value or container was out of the bounds"))]
    PointerOutOfBounds {
      size: usize,
      offset: u32,
      backtrace: snafu::Backtrace,
    },
    #[snafu(display("attempted to get an invalid element: data type is {value:02X}"))]
    InvalidDataType {
      value: u8,
      backtrace: snafu::Backtrace,
    },
    #[snafu(display("expected data type to be {expected:?}, got {actual:?}"))]
    UnexpectedDataType {
      expected: DataType,
      actual: DataType,
      backtrace: snafu::Backtrace,
    },
    #[snafu(display("string table was referenced as an element"))]
    UnexpectedStringTable,
    #[snafu(display("error while deserializing container: {source}"))]
    Container {
      #[snafu(backtrace)]
      source: ContainerError,
    },
    #[snafu(display("failed to retrieve string: {source}"))]
    HashKeyReadError {
      source: StringReadError,
      backtrace: snafu::Backtrace,
    },
    #[snafu(display("{source}"))]
    NonUtf8String {
      source: Utf8Error,
      backtrace: snafu::Backtrace,
    },
  }
}

pub mod write_error {
  use std::{backtrace::Backtrace, io};

  use snafu::{GenerateImplicitData, Snafu};

  #[derive(Snafu, Debug)]
  pub enum WriteError {
    #[snafu(display("error while writing: {source}"))]
    Io {
      source: io::Error,
      backtrace: Backtrace,
    },
    #[snafu(display("overflowed, may be too large to serialize"))]
    Overflowed { backtrace: Backtrace },
  }

  impl From<io::Error> for WriteError {
    #[track_caller]
    fn from(value: io::Error) -> Self {
      WriteError::Io {
        source: value,
        backtrace: Backtrace::generate(),
      }
    }
  }

  pub(super) struct Overflowed;

  impl From<Overflowed> for WriteError {
    fn from(_: Overflowed) -> Self {
      WriteError::Overflowed {
        backtrace: Backtrace::generate(),
      }
    }
  }
}
