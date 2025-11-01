use std::{collections::HashMap, marker::PhantomData};

use snafu::{Backtrace, Snafu};
use zerocopy::{
  little_endian::{U16, U32, U64}, ByteOrder, FromBytes, Immutable, IntoBytes, KnownLayout, LittleEndian
};

use crate::nw::util::{res_dict::DictRef, BinaryBlockHeader, BinaryFileHeader};

#[derive(Snafu, Debug)]
pub enum BfresError {
  #[snafu(display("the header is out of bounds"))]
  HeaderOutOfBounds { backtrace: Backtrace },
}

#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
struct ResFileV8 {
  pub binary_file: BinaryFileHeader<LittleEndian>,
  pub file_name: U64,
  pub models: DictRef<LittleEndian>,
  pub skeletal_anims: DictRef<LittleEndian>,
  pub material_anims: DictRef<LittleEndian>,
  pub bone_vis_anims: DictRef<LittleEndian>,
  pub shape_anims: DictRef<LittleEndian>,
  pub scene_anims: DictRef<LittleEndian>,
  _runtime_memory_pool: U64,
  _runtime_memory_pool_info: U64,
  pub embedded_files: DictRef<LittleEndian>,
  pub _runtime_user_pointer: U64,
  some_string: U64,
  _padding: u32,
  pub model_count: U16,
  pub skeletal_anim_count: U16,
  pub material_anim_count: U16,
  pub bone_vis_anim_count: U16,
  pub shape_anim_count: U16,
  pub scene_anim_count: U16,
  pub embedded_file_count: U16,
  pub external_flag: u8,
  pub _reserved: u8,
}

#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
struct ResModelV8 {
  pub header: BinaryBlockHeader<LittleEndian>,
  pub name: U64,
  _unknown_string: U64,
  pub skeleton_offset: U64,
  pub vertex_buffer_array_offset: U64,
  pub shapes: DictRef<LittleEndian>,
  pub materials: DictRef<LittleEndian>,
  pub user_data: DictRef<LittleEndian>,
  _runtime_user_pointer: U64,
  pub vertex_buffer_count: U16,
  pub shape_count: U16,
  pub material_count: U16,
  pub user_data_count: U16,
  pub total_vertex_count: U32,
  _unused_2: [u8; 4],
}

#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
struct ResSkeletonV8 {
  pub header: BinaryBlockHeader<LittleEndian>,
  pub  bones: DictRef<LittleEndian>,
  pub matrix_to_bone_array_offset: U64,
  pub inverse_model_matrices_offset: U64,
  _unknown: [u8; 16],
  _runtime_user_pointer: U64,
  pub flags: U32,
  pub bone_count: U16,
  pub smooth_matrix_count: U16,
  pub rigid_matrix_count: U16,
}

#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
struct ResBoneV8 {
  pub name: U64,
  pub user_data: DictRef<LittleEndian>,
  pub unknown: [u8; 16]
}

#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
struct ResUserData {
  name_offset: U64,
  data_offset: U64,
  count: U32,
  data_type: U32
}

pub struct Model {
  // pub skeleton:
}

pub struct BfresReaderV8<'a> {
  file_data: &'a [u8],
  pub models: HashMap<&'a str, Model>,
}

const DICT_SIGNATURE: &'static [u8; 4] = b"\0\0\0\0";

impl<'a> BfresReaderV8<'a> {
  pub fn read(file_data: &'a [u8]) -> Result<BfresReaderV8<'a>, BfresError> {
    let file = ResFileV8::read_from_bytes(&file_data[..0xcc]).unwrap();

    let models = file.models.read::<ResModelV8, Model, BfresError>(
      file_data,
      DICT_SIGNATURE,
      |key, model: &ResModelV8| {
        println!("{model:#?}");
        todo!()
        // Ok(())
      },
    ).unwrap();

    Ok(Self { file_data, models })
  }
}
