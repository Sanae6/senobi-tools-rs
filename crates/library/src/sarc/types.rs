use zerocopy::{ByteOrder, FromBytes, Immutable, IntoBytes, KnownLayout, U16, U32};

#[derive(FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct SarcHeader<O: ByteOrder> {
  pub magic: [u8; 4],
  pub header_length: U16<O>,
  pub byte_order_mark: [u8; 2],
  pub file_size: U32<O>,
  pub data_start: U32<O>,
  pub version: U16<O>,
  _reserved: [u8; 2],
}

#[derive(FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct SfatHeader<O: ByteOrder> {
  pub magic: [u8; 4],
  pub header_length: U16<O>,
  pub node_count: U16<O>,
  pub hash_key: U32<O>,
}

#[derive(FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct SfatNode<O: ByteOrder> {
  pub file_name_hash: U32<O>,
  pub file_attributes: U32<O>,
  pub relative_file_start: U32<O>,
  pub relative_file_end: U32<O>,
}

#[derive(FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct SfntHeader<O: ByteOrder> {
  pub magic: [u8; 4],
  pub header_length: U16<O>,
  _reserved: [u8; 2],
}
