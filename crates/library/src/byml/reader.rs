use std::{backtrace::Backtrace, ffi::CStr, fmt::Debug, marker::PhantomData};

use num_traits::FromPrimitive;
use snafu::GenerateImplicitData;
use zerocopy::{ByteOrder, F64, FromBytes, I64, Order as ZCOrder, TryFromBytes, U32, U64};

use crate::{
  byml::{
    ElementReadError, OpenError, Order, StringReadError, StringTableError,
    array_error::ContainerError,
    types::{ContainerHeader, DataType, DictEntry, Header, TryDictEntry},
  },
  util::align_up,
};

#[derive(Clone, Copy)]
struct StringTable<'a, O: ByteOrder> {
  offset_table: &'a [U32<O>],
  start_offset: usize,
  string_data: &'a [u8],
}

impl<'a, O: ByteOrder> StringTable<'a, O> {
  fn get_string_table(data: &'a [u8], offset: u32) -> Result<Self, StringTableError> {
    let usize_offset = offset as usize;
    let header =
      data
        .get(usize_offset..usize_offset + 4)
        .ok_or(StringTableError::HeaderOutOfBounds {
          size: data.len(),
          offset,
          backtrace: Backtrace::generate(),
        })?;
    let header = ContainerHeader::<O>::read_from_bytes(header).unwrap();
    let entries = header.entries();
    let offset_table_end = usize_offset + 4 + (entries as usize * 4);
    let offset_table = data.get(usize_offset + 4..offset_table_end).ok_or(
      StringTableError::AddressTableOutOfBounds {
        size: data.len(),
        offset: offset + 4,
        backtrace: Backtrace::generate(),
      },
    )?;

    let offset_table =
      <[U32<O>]>::ref_from_bytes_with_elems(offset_table, entries as usize).unwrap();

    Ok(Self {
      offset_table,
      string_data: data,
      start_offset: offset as usize,
    })
  }

  fn read_string(&self, index: u32) -> Result<&CStr, StringReadError> {
    let offset = self
      .offset_table
      .get(index as usize)
      .ok_or(StringReadError::OffsetEntryOutOfBounds { offset: index })
      .map(|offset| offset.get() as usize)?
      .checked_add(self.start_offset)
      .ok_or(StringReadError::OffsetOutsideOfStringData)?;

    CStr::from_bytes_until_nul(self.string_data.get(offset..).unwrap())
      .map_err(|_| StringReadError::UnterminatedString)
  }
}

pub enum BymlReader<'a, O: ByteOrder> {
  Array(BymlReaderArray<'a, O>),
  Dictionary(BymlReaderDict<'a, O>),
  Empty,
}

impl<'a, O: ByteOrder> BymlReader<'a, O> {
  pub fn new(data: &'a [u8]) -> Result<Self, OpenError> {
    let header = data
      .get(..size_of::<Header<O>>())
      .ok_or(OpenError::NotEnoughDataForHeader {
        size: data.len(),
        offset: 0,
        backtrace: Backtrace::generate(),
      })?;
    let header = Header::<O>::ref_from_bytes(header).unwrap();

    match (&header.magic, O::ORDER) {
      (b"YB", ZCOrder::LittleEndian) => {}
      (b"BY", ZCOrder::BigEndian) => {}
      (_, ZCOrder::BigEndian) => {
        return Err(OpenError::EndiannessMismatch {
          expected: Order::BigEndian,
          actual: Order::LittleEndian,
          backtrace: Backtrace::generate(),
        });
      }
      (_, ZCOrder::LittleEndian) => {
        return Err(OpenError::EndiannessMismatch {
          expected: Order::LittleEndian,
          actual: Order::BigEndian,
          backtrace: Backtrace::generate(),
        });
      }
    }

    fn get_string_table<'a, O: ByteOrder>(
      offset: u32,
      data: &'a [u8],
      err: impl FnOnce() -> OpenError,
    ) -> Result<Option<StringTable<'a, O>>, OpenError> {
      if offset == 0 {
        Ok(None)
      } else if align_up(offset, 4) == offset {
        Ok(Some(
          StringTable::get_string_table(data, offset)
            .map_err(|source| OpenError::StringTable { source })?,
        ))
      } else {
        return Err(err());
      }
    }

    let string_table = get_string_table(header.string_table_offset.get(), data, || {
      OpenError::StringTableMisaligned {
        size: data.len(),
        offset: header.string_table_offset.get(),
        backtrace: Backtrace::generate(),
      }
    })?;
    let hash_key_table = get_string_table(header.hash_key_offset.get(), data, || {
      OpenError::HashKeyTableMisaligned {
        size: data.len(),
        offset: header.hash_key_offset.get(),
        backtrace: Backtrace::generate(),
      }
    })?;

    let root_node_offset = header.root_node_offset.get();
    let root_node_offset = if root_node_offset == 0 {
      return Ok(BymlReader::Empty);
    } else if align_up(root_node_offset, 4) == root_node_offset {
      root_node_offset
    } else {
      return Err(OpenError::RootNodeMisaligned {
        size: data.len(),
        offset: root_node_offset,
        backtrace: Backtrace::generate(),
      });
    };

    let container_header = data
      .get(root_node_offset as usize..(root_node_offset as usize + 4))
      .ok_or(OpenError::RootNodeOutOfBounds {
        size: data.len(),
        offset: root_node_offset,
        backtrace: Backtrace::generate(),
      })?;
    let container_header = ContainerHeader::<O>::read_from_bytes(container_header).unwrap();

    let data_type =
      DataType::from_u8(container_header.data_type).ok_or(OpenError::InvalidDataType {
        value: container_header.data_type,
        backtrace: Backtrace::generate(),
      })?;

    match data_type {
      DataType::Array => {
        let (data_types, values) = BymlReaderArray::get_components(
          data,
          container_header.entries(),
          root_node_offset as usize,
        )
        .map_err(|source| OpenError::Container { source })?;
        Ok(Self::Array(BymlReaderArray {
          data,
          string_table,
          hash_key_table,
          data_types,
          values,
          _p: PhantomData,
        }))
      }
      DataType::Dictionary => {
        let (entries, hash_key_table) = BymlReaderDict::<O>::get_components(
          data,
          container_header.entries(),
          root_node_offset as usize,
          hash_key_table.as_ref(),
        )
        .map_err(|source| OpenError::Container { source })?;

        Ok(BymlReader::Dictionary(BymlReaderDict {
          data,
          string_table,
          hash_key_table,
          entries,
          _p: PhantomData,
        }))
      }
      _ => Err(OpenError::NonContainerType {
        value: data_type,
        backtrace: Backtrace::generate(),
      }),
    }
  }

  pub fn unwrap_array(self) -> BymlReaderArray<'a, O> {
    let BymlReader::Array(array) = self else {
      panic!("unwrapped a non array type")
    };

    array
  }
}

macro_rules! getter_impls {
  (
    [$ty: ty, $param: ident: $param_ty: ty]
    $(($func: ident, $ret_ty: ty, $variant: ident)),*
  ) => {
    impl<'a, O: ByteOrder> $ty {
      $(
        pub fn $func(&'a self, $param: $param_ty) -> Result<Option<$ret_ty>, ElementReadError> {
          match self.get_element($param)? {
            Some(BymlReaderNode::$variant(value)) => Ok(Some(value)),
            Some(element) => Err(ElementReadError::UnexpectedDataType {
              expected: DataType::$variant,
              actual: element.data_type(),
              backtrace: Backtrace::generate()
            }),
            None => Ok(None)
          }
        }
      )*
    }
  };
}

pub struct BymlReaderArray<'a, O: ByteOrder> {
  data: &'a [u8],
  string_table: Option<StringTable<'a, O>>,
  hash_key_table: Option<StringTable<'a, O>>,
  data_types: &'a [DataType],
  values: &'a [U32<O>],
  _p: PhantomData<O>,
}

impl<'a, O: ByteOrder> BymlReaderArray<'a, O> {
  fn get_components(
    data: &[u8],
    entries: u32,
    start: usize,
  ) -> Result<(&[DataType], &[U32<O>]), ContainerError> {
    let entries_end = start + 4 + entries as usize;

    let data_types =
      data
        .get(start + 4..entries_end)
        .ok_or(ContainerError::DataTypesOutOfBounds {
          size: data.len(),
          offset: start as u32 + 4,
          backtrace: Backtrace::generate(),
        })?;

    data_types.iter().enumerate().try_for_each(
      |(index, data_type)| -> Result<(), ContainerError> {
        DataType::from_u8(*data_type).ok_or(ContainerError::InvalidElementDataType {
          element_index: index,
          value: *data_type,
          backtrace: Backtrace::generate(),
        })?;

        Ok(())
      },
    )?;

    let data_types =
      <[DataType]>::try_ref_from_bytes_with_elems(data_types, entries as usize).unwrap();

    let values_start = align_up(entries_end, 4);

    let values = data
      .get(values_start..values_start + entries as usize * 4)
      .ok_or(ContainerError::ValuesOutOfBounds {
        size: data.len(),
        offset: values_start as u32,
        backtrace: Backtrace::generate(),
      })?;

    let values = <[U32<O>]>::ref_from_bytes_with_elems(values, entries as usize).unwrap();

    Ok((data_types, values))
  }

  pub fn get_element(
    &'a self,
    index: u32,
  ) -> Result<Option<BymlReaderNode<'a, O>>, ElementReadError> {
    let Some(data_type) = self.data_types.get(index as usize) else {
      return Ok(None);
    };

    let value = self.values.get(index as usize).unwrap().get();

    let read_from_pointer = |size: usize| -> Result<&[u8], ElementReadError> {
      self
        .data
        .get(value as usize..(value as usize + size))
        .ok_or(ElementReadError::ValueOutOfBounds {
          size: self.data.len(),
          offset: value,
          backtrace: Backtrace::generate(),
        })
    };

    match data_type {
      DataType::String => {
        let string = self
          .string_table
          .as_ref()
          .ok_or(ElementReadError::NoStringTable {
            backtrace: Backtrace::generate(),
          })?
          .read_string(value)
          .map_err(|source| ElementReadError::StringReadError {
            source,
            backtrace: Backtrace::generate(),
          })?;

        Ok(Some(BymlReaderNode::<O>::String(string)))
      }
      DataType::Array => {
        let container_header =
          ContainerHeader::<O>::read_from_bytes(read_from_pointer(4)?).unwrap();

        let (data_types, values) =
          BymlReaderArray::get_components(self.data, container_header.entries(), value as usize)
            .map_err(|source| ElementReadError::Container { source })?;

        Ok(Some(BymlReaderNode::Array(BymlReaderArray {
          data: self.data,
          string_table: self.string_table,
          hash_key_table: self.hash_key_table,
          data_types,
          values,
          _p: PhantomData,
        })))
      }
      DataType::Dictionary => {
        let container_header =
          ContainerHeader::<O>::read_from_bytes(read_from_pointer(4)?).unwrap();

        let (entries, hash_key_table) = BymlReaderDict::<O>::get_components(
          self.data,
          container_header.entries(),
          value as usize,
          self.hash_key_table.as_ref(),
        )
        .map_err(|source| ElementReadError::Container { source })?;

        Ok(Some(BymlReaderNode::Dictionary(BymlReaderDict {
          data: self.data,
          string_table: self.string_table,
          hash_key_table: hash_key_table,
          entries,
          _p: PhantomData,
        })))
      }
      DataType::StringTable => Err(ElementReadError::UnexpectedStringTable),
      DataType::Bool => Ok(Some(BymlReaderNode::<O>::Bool(value > 0))),
      DataType::I32 => Ok(Some(BymlReaderNode::<O>::I32(i32::from_ne_bytes(
        value.to_ne_bytes(),
      )))),
      DataType::F32 => Ok(Some(BymlReaderNode::<O>::F32(f32::from_ne_bytes(
        value.to_ne_bytes(),
      )))),
      DataType::U32 => Ok(Some(BymlReaderNode::<O>::U32(u32::from_ne_bytes(
        value.to_ne_bytes(),
      )))),
      DataType::I64 => {
        let value = read_from_pointer(8)?;
        let value = I64::<O>::read_from_bytes(value).unwrap();

        Ok(Some(BymlReaderNode::<O>::I64(value.get())))
      }
      DataType::U64 => {
        let value = read_from_pointer(8)?;
        let value = U64::<O>::read_from_bytes(value).unwrap();

        Ok(Some(BymlReaderNode::<O>::U64(value.get())))
      }
      DataType::F64 => {
        let value = read_from_pointer(8)?;
        let value = F64::<O>::read_from_bytes(value).unwrap();

        Ok(Some(BymlReaderNode::<O>::F64(value.get())))
      }
      DataType::Null => Ok(Some(BymlReaderNode::Null)),
    }
  }

  pub fn values(&'_ self) -> impl Iterator<Item = Result<BymlReaderNode<'_, O>, ElementReadError>> {
    (0..self.data_types.len()).map(|index| self.get_element(index as u32).transpose().unwrap())
  }
}

getter_impls! {
  [BymlReaderArray<'a, O>, index: u32]
  (get_array, BymlReaderArray<'a, O>, Array),
  (get_dict, BymlReaderDict<'a, O>, Dictionary),
  (get_bool, bool, Bool),
  (get_i32, i32, I32),
  (get_u32, u32, U32),
  (get_f32, f32, F32),
  (get_i64, i64, I64),
  (get_u64, u64, U64),
  (get_f64, f64, F64),
  (get_cstring, &'a CStr, String)
}

impl<'a, O: ByteOrder> Debug for BymlReaderArray<'a, O> {
  fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
    self.values().collect::<Result<Vec<_>, _>>().fmt(f)
  }
}

pub struct BymlReaderDict<'a, O: ByteOrder> {
  data: &'a [u8],
  string_table: Option<StringTable<'a, O>>,
  hash_key_table: StringTable<'a, O>,
  entries: &'a [DictEntry<O>],
  _p: PhantomData<O>,
}

impl<'a, O: ByteOrder> BymlReaderDict<'a, O> {
  fn get_components(
    data: &'a [u8],
    entries: u32,
    start: usize,
    hash_key_table: Option<&StringTable<'a, O>>,
  ) -> Result<(&'a [DictEntry<O>], StringTable<'a, O>), ContainerError> {
    let Some(hash_key_table) = hash_key_table else {
      return Err(ContainerError::NoHashKeyTable {
        backtrace: Backtrace::generate(),
      });
    };

    let entries_end = start + 4 + entries as usize * size_of::<DictEntry<O>>();

    let dict_entries =
      data
        .get(start + 4..entries_end)
        .ok_or(ContainerError::DataTypesOutOfBounds {
          size: data.len(),
          offset: start as u32 + 4,
          backtrace: Backtrace::generate(),
        })?;

    let try_dict_entries =
      <[TryDictEntry<O>]>::ref_from_bytes_with_elems(dict_entries, entries as usize).unwrap();

    try_dict_entries.iter().enumerate().try_for_each(
      |(index, entry)| -> Result<(), ContainerError> {
        DataType::from_u8(entry.data_type).ok_or(ContainerError::InvalidElementDataType {
          element_index: index,
          value: entry.data_type,
          backtrace: Backtrace::generate(),
        })?;

        Ok(())
      },
    )?;

    let dict_entries =
      <[DictEntry<O>]>::try_ref_from_bytes_with_elems(dict_entries, entries as usize).unwrap();

    Ok((dict_entries, *hash_key_table))
  }

  pub fn get_element(
    &'a self,
    index: &str,
  ) -> Result<Option<BymlReaderNode<'a, O>>, ElementReadError> {
    self.get_element_by_key_bytes(index.as_bytes())
  }

  pub fn get_element_by_key_bytes(
    &'a self,
    index: &[u8],
  ) -> Result<Option<BymlReaderNode<'a, O>>, ElementReadError> {
    let Some((value, data_type)) = self.get_entry_by_key_bytes(index)? else {
      return Ok(None);
    };

    self.get_element_from_entry(value, data_type)
  }

  fn get_entry_by_key_bytes(
    &self,
    index: &[u8],
  ) -> Result<Option<(u32, DataType)>, ElementReadError> {
    // try_binary_search_by doesn't exist, unfortunately
    let mut low = 0;
    let mut high = self.entries.len() - 1;
    let mut found_entry = None;

    while low <= high {
      let mid = (low + high) / 2;
      let entry = &self.entries[mid];
      let value = self
        .hash_key_table
        .read_string(entry.hash_key_index())
        .map_err(|source| ElementReadError::HashKeyReadError {
          source,
          backtrace: Backtrace::generate(),
        })?;

      let ordering = index
        .partial_cmp(&value.to_bytes())
        .expect("invalid partial comparison between two byte ");
      match ordering {
        std::cmp::Ordering::Less => {
          high = if let Some(new_high) = mid.checked_sub(1) {
            new_high
          } else {
            return Ok(None);
          }
        }
        std::cmp::Ordering::Equal => {
          found_entry = Some(entry);
          break;
        }
        std::cmp::Ordering::Greater => low = mid + 1,
      }
    }

    Ok(found_entry.map(
      |DictEntry {
         data_type, value, ..
       }| (value.get(), *data_type),
    ))
  }

  fn get_element_from_entry(
    &'_ self,
    value: u32,
    data_type: DataType,
  ) -> Result<Option<BymlReaderNode<'_, O>>, ElementReadError> {
    let read_from_pointer = |size: usize| -> Result<&[u8], ElementReadError> {
      self
        .data
        .get(value as usize..(value as usize + size))
        .ok_or(ElementReadError::ValueOutOfBounds {
          size: self.data.len(),
          offset: value,
          backtrace: Backtrace::generate(),
        })
    };

    match data_type {
      DataType::String => {
        let string = self
          .string_table
          .as_ref()
          .ok_or(ElementReadError::NoStringTable {
            backtrace: Backtrace::generate(),
          })?
          .read_string(value)
          .map_err(|source| ElementReadError::StringReadError {
            source,
            backtrace: Backtrace::generate(),
          })?;

        Ok(Some(BymlReaderNode::<O>::String(string)))
      }
      DataType::Array => {
        let container_header =
          ContainerHeader::<O>::read_from_bytes(read_from_pointer(4)?).unwrap();

        let (data_types, values) =
          BymlReaderArray::get_components(self.data, container_header.entries(), value as usize)
            .map_err(|source| ElementReadError::Container { source })?;

        Ok(Some(BymlReaderNode::Array(BymlReaderArray {
          data: self.data,
          string_table: self.string_table,
          hash_key_table: Some(self.hash_key_table),
          data_types,
          values,
          _p: PhantomData,
        })))
      }
      DataType::Dictionary => {
        let container_header =
          ContainerHeader::<O>::read_from_bytes(read_from_pointer(4)?).unwrap();

        let (entries, _) = BymlReaderDict::<O>::get_components(
          self.data,
          container_header.entries(),
          value as usize,
          Some(&self.hash_key_table),
        )
        .map_err(|source| ElementReadError::Container { source })?;

        Ok(Some(BymlReaderNode::Dictionary(BymlReaderDict {
          data: self.data,
          string_table: self.string_table,
          hash_key_table: self.hash_key_table,
          entries,
          _p: PhantomData,
        })))
      }
      DataType::StringTable => Err(ElementReadError::UnexpectedStringTable),
      DataType::Bool => Ok(Some(BymlReaderNode::<O>::Bool(value > 0))),
      DataType::I32 => Ok(Some(BymlReaderNode::<O>::I32(i32::from_ne_bytes(
        value.to_ne_bytes(),
      )))),
      DataType::F32 => Ok(Some(BymlReaderNode::<O>::F32(f32::from_ne_bytes(
        value.to_ne_bytes(),
      )))),
      DataType::U32 => Ok(Some(BymlReaderNode::<O>::U32(u32::from_ne_bytes(
        value.to_ne_bytes(),
      )))),
      DataType::I64 => {
        let value = read_from_pointer(8)?;
        let value = I64::<O>::read_from_bytes(value).unwrap();

        Ok(Some(BymlReaderNode::<O>::I64(value.get())))
      }
      DataType::U64 => {
        let value = read_from_pointer(8)?;
        let value = U64::<O>::read_from_bytes(value).unwrap();

        Ok(Some(BymlReaderNode::<O>::U64(value.get())))
      }
      DataType::F64 => {
        let value = read_from_pointer(8)?;
        let value = F64::<O>::read_from_bytes(value).unwrap();

        Ok(Some(BymlReaderNode::<O>::F64(value.get())))
      }
      DataType::Null => Ok(Some(BymlReaderNode::Null)),
    }
  }

  pub fn cstr_keys(&self) -> impl Iterator<Item = Result<&CStr, StringReadError>> {
    self
      .entries
      .iter()
      .map(|entry| self.hash_key_table.read_string(entry.hash_key_index()))
  }

  pub fn keys(&self) -> impl Iterator<Item = Result<&str, StringReadError>> {
    self.entries.iter().map(|entry| {
      self
        .hash_key_table
        .read_string(entry.hash_key_index())
        .and_then(|value| {
          value
            .to_str()
            .map_err(|error| StringReadError::NonUtf8String { error })
        })
    })
  }

  pub fn cstr_entries(
    &self,
  ) -> impl Iterator<Item = Result<(&CStr, BymlReaderNode<'_, O>), ElementReadError>> {
    (0..self.entries.len()).map(|index| -> Result<_, ElementReadError> {
      let string = self
        .hash_key_table
        .read_string(self.entries[index].hash_key_index())
        .map_err(|source| ElementReadError::HashKeyReadError {
          source,
          backtrace: Backtrace::generate(),
        })?;
      Ok((
        string,
        self.get_element_by_key_bytes(string.to_bytes())?.unwrap(),
      ))
    })
  }

  pub fn entries(
    &self,
  ) -> impl Iterator<Item = Result<(&str, BymlReaderNode<'_, O>), ElementReadError>> {
    (0..self.entries.len()).map(|index| -> Result<_, ElementReadError> {
      let string = self
        .hash_key_table
        .read_string(self.entries[index].hash_key_index())
        .map_err(|source| ElementReadError::HashKeyReadError {
          source,
          backtrace: Backtrace::generate(),
        })?;
      let string = string
        .to_str()
        .map_err(|source| ElementReadError::NonUtf8String {
          source,
          backtrace: Backtrace::generate(),
        })?;
      Ok((
        string,
        self.get_element_by_key_bytes(string.as_bytes())?.unwrap(),
      ))
    })
  }

  pub fn get_string(&'a self, key: &str) -> Result<Option<&'a str>, ElementReadError> {
    let Some(value) = self.get_cstring(key)? else {
      return Ok(None);
    };

    value
      .to_str()
      .map(Some)
      .map_err(|error| ElementReadError::NonUtf8String {
        source: error,
        backtrace: Backtrace::generate(),
      })
  }

  pub fn get_type(&self, key: &str) -> Result<Option<DataType>, ElementReadError> {
    self
      .get_entry_by_key_bytes(key.as_bytes())
      .map(|value| value.map(|(_, data_type)| data_type))
  }
}

getter_impls! {
  [BymlReaderDict<'a, O>, key: &'_ str]
  (get_array, BymlReaderArray<'a, O>, Array),
  (get_dict, BymlReaderDict<'a, O>, Dictionary),
  (get_bool, bool, Bool),
  (get_i32, i32, I32),
  (get_u32, u32, U32),
  (get_f32, f32, F32),
  (get_i64, i64, I64),
  (get_u64, u64, U64),
  (get_f64, f64, F64),
  (get_cstring, &'a CStr, String)
}

impl<'a, O: ByteOrder> Debug for BymlReaderDict<'a, O> {
  fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
    self.entries().collect::<Result<Vec<_>, _>>().fmt(f)
  }
}

#[derive(Debug)]
pub enum BymlReaderNode<'a, O: ByteOrder> {
  Array(BymlReaderArray<'a, O>),
  Dictionary(BymlReaderDict<'a, O>),
  Bool(bool),
  I32(i32),
  F32(f32),
  U32(u32),
  I64(i64),
  U64(u64),
  F64(f64),
  String(&'a CStr),
  Null,
}

impl<'a, O: ByteOrder> BymlReaderNode<'a, O> {
  fn data_type(&self) -> DataType {
    match self {
      BymlReaderNode::Array(_) => DataType::Array,
      BymlReaderNode::Dictionary(_) => DataType::Dictionary,
      BymlReaderNode::Bool(_) => DataType::Bool,
      BymlReaderNode::I32(_) => DataType::I32,
      BymlReaderNode::F32(_) => DataType::F32,
      BymlReaderNode::U32(_) => DataType::U32,
      BymlReaderNode::I64(_) => DataType::I64,
      BymlReaderNode::U64(_) => DataType::U64,
      BymlReaderNode::F64(_) => DataType::F64,
      BymlReaderNode::String(_) => DataType::String,
      BymlReaderNode::Null => DataType::Null,
    }
  }
}
