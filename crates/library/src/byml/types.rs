use std::marker::PhantomData;

use num_derive::FromPrimitive;
use zerocopy::{
  ByteOrder, FromBytes, Immutable, IntoBytes, KnownLayout, TryFromBytes, U16, U32, Unaligned,
};

#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct Header<O: ByteOrder> {
  pub magic: [u8; 2],
  pub version: U16<O>,
  pub hash_key_offset: U32<O>,
  pub string_table_offset: U32<O>,
  pub root_node_offset: U32<O>,
}

#[derive(FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct ContainerHeader<O: ByteOrder> {
  pub data_type: u8,
  entries: [u8; 3],
  _p: PhantomData<O>,
}

impl<O: ByteOrder> ContainerHeader<O> {
  pub fn entries(&self) -> u32 {
    match O::ORDER {
      zerocopy::Order::BigEndian => {
        let [a, b, c] = self.entries;
        u32::from_be_bytes([0, a, b, c])
      }
      zerocopy::Order::LittleEndian => {
        let [a, b, c] = self.entries;
        u32::from_le_bytes([a, b, c, 0])
      }
    }
  }
}

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable)]
#[repr(C)]
pub struct TryDictEntry<O: ByteOrder> {
  pub hash_key_index: [u8; 3],
  pub data_type: u8,
  pub value: U32<O>,
}

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable)]
#[repr(C)]
pub struct DictEntry<O: ByteOrder> {
  pub hash_key_index: [u8; 3],
  pub data_type: DataType,
  pub value: U32<O>,
}

impl<O: ByteOrder> DictEntry<O> {
  pub fn hash_key_index(&self) -> u32 {
    match O::ORDER {
      zerocopy::Order::BigEndian => {
        let [a, b, c] = self.hash_key_index;
        u32::from_be_bytes([0, a, b, c])
      }
      zerocopy::Order::LittleEndian => {
        let [a, b, c] = self.hash_key_index;
        u32::from_le_bytes([a, b, c, 0])
      }
    }
  }
}

#[derive(
  Debug, FromPrimitive, TryFromBytes, IntoBytes, Unaligned, Immutable, PartialEq, Eq, Clone, Copy,
)]
#[repr(u8)]
pub enum DataType {
  String = 0xA0,
  Array = 0xC0,
  Dictionary = 0xC1,
  StringTable = 0xC2,
  Bool = 0xD0,
  I32 = 0xD1,
  F32 = 0xD2,
  U32 = 0xD3,
  I64 = 0xD4,
  U64 = 0xD5,
  F64 = 0xD6,
  Null = 0xFF,
}
