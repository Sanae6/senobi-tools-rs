pub mod iter;
mod types;

pub const MAXIMUM_SUPPORTED_VERSION: u16 = 3;

#[derive(Debug)]
pub enum Order {
  LittleEndian,
  BigEndian,
}

use string_table_error::StringTableError;
mod string_table_error {
  use snafu::Snafu;

  #[derive(Snafu, Debug)]
  pub enum StringTableError {
    HeaderOutOfBounds { size: usize, offset: u32 },
    AddressTableOutOfBounds { size: usize, offset: u32 },
    StringDataEndOutOfBounds { size: usize, offset: u32 },
  }
}

mod array_error {
  use snafu::Snafu;

  #[derive(Snafu, Debug)]
  pub enum ContainerError {
    #[snafu(display("not an array, got data type {value:02X}"))]
    IncorrectDataType {
      value: u8,
    },
    #[snafu(display("array data type list was out of bounds"))]
    DataTypesOutOfBounds {
      size: usize,
      offset: u32,
    },
    #[snafu(display("not an array, got data type {value:02X}"))]
    InvalidElementDataType {
      element_index: usize,
      value: u8,
    },
    #[snafu(display("array value list was out of bounds"))]
    ValuesOutOfBounds {
      size: usize,
      offset: u32,
    },
    NoHashKeyTable,
  }
}

use open_error::OpenError;
mod open_error {
  use snafu::Snafu;

  use crate::byml::{Order, StringTableError, array_error::ContainerError, types::DataType};

  #[derive(Snafu, Debug)]
  pub enum OpenError {
    #[snafu(display("expected byml endianness to be {expected:?}, got {actual:?}"))]
    EndiannessMismatch { expected: Order, actual: Order },
    #[snafu(display("unsupported version {actual}, greatest supported version is {maximum}"))]
    UnsupportedVersion { maximum: u16, actual: u16 },
    #[snafu(display("attempted to read out of bounds: size is {size} but offset is {offset}"))]
    NotEnoughDataForHeader { size: usize, offset: u32 },
    #[snafu(display("root node points out of bounds"))]
    RootNodeOutOfBounds { size: usize, offset: u32 },
    #[snafu(display("root node pointer is misaligned"))]
    RootNodeMisaligned { size: usize, offset: u32 },
    #[snafu(display("string table points out of bounds"))]
    StringTableOutOfBounds { size: usize, offset: u32 },
    #[snafu(display("string table pointer is misaligned"))]
    StringTableMisaligned { size: usize, offset: u32 },
    #[snafu(display("string table could not be deserialized: {error}"))]
    StringTable { error: StringTableError },
    #[snafu(display("hash key table points out of bounds"))]
    HashKeyTableOutOfBounds { size: usize, offset: u32 },
    #[snafu(display("string table pointer is misaligned"))]
    HashKeyTableMisaligned { size: usize, offset: u32 },
    #[snafu(display("hash key table table could not be deserialized: {error}"))]
    HashKeyTable { error: StringTableError },
    #[snafu(display("root node was not a valid data type: data type is {value:02X}"))]
    InvalidDataType { value: u8 },
    #[snafu(display("root type was not a container type: data type is {value:?}"))]
    NonContainerType { value: DataType },
    #[snafu(display("error while deserializing array: {error}"))]
    Container { error: ContainerError },
  }
}

use string_read_error::StringReadError;
mod string_read_error {
  use snafu::Snafu;

  #[derive(Snafu, Debug)]
  pub enum StringReadError {
    OffsetEntryOutOfBounds { offset: u32 },
    OffsetOutsideOfStringData,
    UnterminatedString,
  }
}

use element_error::ElementError;
mod element_error {
  use snafu::Snafu;

  use crate::byml::{StringReadError, array_error::ContainerError, types::DataType};

  #[derive(Snafu, Debug)]
  pub enum ElementError {
    #[snafu(display("failed to retrieve string: {error}"))]
    StringReadError { error: StringReadError },
    #[snafu(display("no string table when string was requested"))]
    NoStringTable,
    #[snafu(display("no hash key table when dictionary was requested"))]
    NoHashKeyTable,
    #[snafu(display("value is out of bounds"))]
    ValueOutOfBounds { size: usize, offset: u32 },
    #[snafu(display("pointer for value or container was out of the bounds"))]
    PointerOutOfBounds { size: usize, offset: u32 },
    #[snafu(display("attempted to get an invalid element: data type is {value:02X}"))]
    InvalidDataType { value: u8 },
    #[snafu(display("expected data type to be {expected:?}, got {actual:?}"))]
    UnexpectedDataType {
      expected: DataType,
      actual: DataType,
    },
    #[snafu(display("string table was referenced as an element"))]
    UnexpectedStringTable,
    #[snafu(display("error while deserializing container: {error}"))]
    Array { error: ContainerError },
    #[snafu(display("failed to retrieve string: {error}"))]
    HashKeyReadError { error: StringReadError },
  }
}
