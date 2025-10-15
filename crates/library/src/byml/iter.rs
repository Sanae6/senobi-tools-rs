use std::{ffi::CStr, fmt::Debug, marker::PhantomData};

use num_traits::FromPrimitive;
use zerocopy::{ByteOrder, F64, FromBytes, I64, Order as ZCOrder, TryFromBytes, U32, U64};

use crate::{
  byml::{
    ElementError, OpenError, Order, StringReadError, StringTableError,
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
        })?;
    let header = ContainerHeader::<O>::read_from_bytes(header).unwrap();
    let entries = header.entries();
    let offset_table_end = usize_offset + 4 + (entries as usize * 4);
    let offset_table = data.get(usize_offset + 4..offset_table_end).ok_or(
      StringTableError::AddressTableOutOfBounds {
        size: data.len(),
        offset: offset + 4,
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

pub enum BymlContainer<'a, O: ByteOrder> {
  Array(BymlArrayIter<'a, O>),
  Dictionary(BymlDictIter<'a, O>),
  Empty,
}

impl<'a, O: ByteOrder> BymlContainer<'a, O> {
  pub fn new(data: &'a [u8]) -> Result<Self, OpenError> {
    let header = data
      .get(..size_of::<Header<O>>())
      .ok_or(OpenError::NotEnoughDataForHeader {
        size: data.len(),
        offset: 0,
      })?;
    let header = Header::<O>::ref_from_bytes(header).unwrap();

    match (&header.magic, O::ORDER) {
      (b"YB", ZCOrder::LittleEndian) => {}
      (b"BY", ZCOrder::BigEndian) => {}
      (_, ZCOrder::BigEndian) => {
        return Err(OpenError::EndiannessMismatch {
          expected: Order::BigEndian,
          actual: Order::LittleEndian,
        });
      }
      (_, ZCOrder::LittleEndian) => {
        return Err(OpenError::EndiannessMismatch {
          expected: Order::LittleEndian,
          actual: Order::BigEndian,
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
            .map_err(|error| OpenError::StringTable { error })?,
        ))
      } else {
        return Err(err());
      }
    }

    let string_table = get_string_table(header.string_table_offset.get(), data, || {
      OpenError::StringTableMisaligned {
        size: data.len(),
        offset: header.string_table_offset.get(),
      }
    })?;
    let hash_key_table = get_string_table(header.hash_key_offset.get(), data, || {
      OpenError::HashKeyTableMisaligned {
        size: data.len(),
        offset: header.hash_key_offset.get(),
      }
    })?;

    let root_node_offset = header.root_node_offset.get();
    let root_node_offset = if root_node_offset == 0 {
      return Ok(BymlContainer::Empty);
    } else if align_up(root_node_offset, 4) == root_node_offset {
      root_node_offset
    } else {
      return Err(OpenError::RootNodeMisaligned {
        size: data.len(),
        offset: root_node_offset,
      });
    };

    let container_header = data
      .get(root_node_offset as usize..(root_node_offset as usize + 4))
      .ok_or(OpenError::RootNodeOutOfBounds {
        size: data.len(),
        offset: root_node_offset,
      })?;
    let container_header = ContainerHeader::<O>::read_from_bytes(container_header).unwrap();

    let data_type =
      DataType::from_u8(container_header.data_type).ok_or(OpenError::InvalidDataType {
        value: container_header.data_type,
      })?;

    match data_type {
      DataType::Array => {
        let (data_types, values) = BymlArrayIter::get_components(
          data,
          container_header.entries(),
          root_node_offset as usize,
        )
        .map_err(|error| OpenError::Container { error })?;
        Ok(Self::Array(BymlArrayIter {
          data,
          string_table,
          hash_key_table,
          data_types,
          values,
          _p: PhantomData,
        }))
      }
      DataType::Dictionary => {
        let (entries, hash_key_table) = BymlDictIter::<O>::get_components(
          data,
          container_header.entries(),
          root_node_offset as usize,
          hash_key_table.as_ref(),
        )
        .map_err(|error| OpenError::Container { error })?;

        Ok(BymlContainer::Dictionary(BymlDictIter {
          data,
          string_table,
          hash_key_table,
          entries,
          _p: PhantomData,
        }))
      }
      _ => Err(OpenError::NonContainerType { value: data_type }),
    }
  }

  pub fn unwrap_array(self) -> BymlArrayIter<'a, O> {
    let BymlContainer::Array(array) = self else {
      panic!("unwrapped a non array type")
    };

    array
  }
}

pub struct BymlArrayIter<'a, O: ByteOrder> {
  data: &'a [u8],
  string_table: Option<StringTable<'a, O>>,
  hash_key_table: Option<StringTable<'a, O>>,
  data_types: &'a [DataType],
  values: &'a [U32<O>],
  _p: PhantomData<O>,
}

impl<'a, O: ByteOrder> BymlArrayIter<'a, O> {
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
        })?;

    data_types.iter().enumerate().try_for_each(
      |(index, data_type)| -> Result<(), ContainerError> {
        DataType::from_u8(*data_type).ok_or(ContainerError::InvalidElementDataType {
          element_index: index,
          value: *data_type,
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
      })?;

    let values = <[U32<O>]>::ref_from_bytes_with_elems(values, entries as usize).unwrap();

    Ok((data_types, values))
  }

  pub fn get_element(&'a self, index: u32) -> Result<Option<BymlElement<'a, O>>, ElementError> {
    let Some(data_type) = self.data_types.get(index as usize) else {
      return Ok(None);
    };

    let value = self.values.get(index as usize).unwrap().get();

    let read_from_pointer = |size: usize| -> Result<&[u8], ElementError> {
      self
        .data
        .get(value as usize..(value as usize + size))
        .ok_or(ElementError::ValueOutOfBounds {
          size: self.data.len(),
          offset: value,
        })
    };

    match data_type {
      DataType::String => {
        let string = self
          .string_table
          .as_ref()
          .ok_or(ElementError::NoStringTable)?
          .read_string(value)
          .map_err(|error| ElementError::StringReadError { error })?;

        Ok(Some(BymlElement::<O>::String(string)))
      }
      DataType::Array => {
        let container_header =
          ContainerHeader::<O>::read_from_bytes(read_from_pointer(4)?).unwrap();

        let (data_types, values) =
          BymlArrayIter::get_components(self.data, container_header.entries(), value as usize)
            .map_err(|error| ElementError::Array { error })?;

        Ok(Some(BymlElement::Array(BymlArrayIter {
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

        let (entries, hash_key_table) = BymlDictIter::<O>::get_components(
          self.data,
          container_header.entries(),
          value as usize,
          self.hash_key_table.as_ref(),
        )
        .map_err(|error| ElementError::Array { error })?;

        Ok(Some(BymlElement::Dict(BymlDictIter {
          data: self.data,
          string_table: self.string_table,
          hash_key_table: hash_key_table,
          entries,
          _p: PhantomData,
        })))
      }
      DataType::StringTable => Err(ElementError::UnexpectedStringTable),
      DataType::Bool => Ok(Some(BymlElement::<O>::Bool(value > 0))),
      DataType::I32 => Ok(Some(BymlElement::<O>::I32(i32::from_ne_bytes(
        value.to_ne_bytes(),
      )))),
      DataType::F32 => Ok(Some(BymlElement::<O>::F32(f32::from_ne_bytes(
        value.to_ne_bytes(),
      )))),
      DataType::U32 => Ok(Some(BymlElement::<O>::U32(u32::from_ne_bytes(
        value.to_ne_bytes(),
      )))),
      DataType::I64 => {
        let value = read_from_pointer(8)?;
        let value = I64::<O>::read_from_bytes(value).unwrap();

        Ok(Some(BymlElement::<O>::I64(value.get())))
      }
      DataType::U64 => {
        let value = read_from_pointer(8)?;
        let value = U64::<O>::read_from_bytes(value).unwrap();

        Ok(Some(BymlElement::<O>::U64(value.get())))
      }
      DataType::F64 => {
        let value = read_from_pointer(8)?;
        let value = F64::<O>::read_from_bytes(value).unwrap();

        Ok(Some(BymlElement::<O>::F64(value.get())))
      }
      DataType::Null => Ok(Some(BymlElement::Null)),
    }
  }

  pub fn values(&'_ self) -> impl Iterator<Item = Result<BymlElement<'_, O>, ElementError>> {
    (0..self.data_types.len()).map(|index| self.get_element(index as u32).transpose().unwrap())
  }
}

impl<'a, O: ByteOrder> Debug for BymlArrayIter<'a, O> {
  fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
    self.values().collect::<Result<Vec<_>, _>>().fmt(f)
  }
}

pub struct BymlDictIter<'a, O: ByteOrder> {
  data: &'a [u8],
  string_table: Option<StringTable<'a, O>>,
  hash_key_table: StringTable<'a, O>,
  entries: &'a [DictEntry<O>],
  _p: PhantomData<O>,
}

impl<'a, O: ByteOrder> BymlDictIter<'a, O> {
  fn get_components(
    data: &'a [u8],
    entries: u32,
    start: usize,
    hash_key_table: Option<&StringTable<'a, O>>,
  ) -> Result<(&'a [DictEntry<O>], StringTable<'a, O>), ContainerError> {
    let Some(hash_key_table) = hash_key_table else {
      return Err(ContainerError::NoHashKeyTable);
    };

    let entries_end = start + 4 + entries as usize * size_of::<DictEntry<O>>();

    let dict_entries =
      data
        .get(start + 4..entries_end)
        .ok_or(ContainerError::DataTypesOutOfBounds {
          size: data.len(),
          offset: start as u32 + 4,
        })?;

    let try_dict_entries =
      <[TryDictEntry<O>]>::ref_from_bytes_with_elems(dict_entries, entries as usize).unwrap();

    try_dict_entries.iter().enumerate().try_for_each(
      |(index, entry)| -> Result<(), ContainerError> {
        DataType::from_u8(entry.data_type).ok_or(ContainerError::InvalidElementDataType {
          element_index: index,
          value: entry.data_type,
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
    index: impl PartialOrd<&'a [u8]>,
  ) -> Result<Option<BymlElement<'a, O>>, ElementError> {
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
        .map_err(|error| ElementError::HashKeyReadError { error })?;

      let ordering = index.partial_cmp(&value.to_bytes()).expect("actually no");
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

    let Some(DictEntry {
      data_type, value, ..
    }) = found_entry
    else {
      return Ok(None);
    };

    self.get_element_from_entry(value.get(), *data_type)
  }

  fn get_element_from_entry(
    &'_ self,
    value: u32,
    data_type: DataType,
  ) -> Result<Option<BymlElement<'_, O>>, ElementError> {
    let read_from_pointer = |size: usize| -> Result<&[u8], ElementError> {
      self
        .data
        .get(value as usize..(value as usize + size))
        .ok_or(ElementError::ValueOutOfBounds {
          size: self.data.len(),
          offset: value,
        })
    };

    match data_type {
      DataType::String => {
        let string = self
          .string_table
          .as_ref()
          .ok_or(ElementError::NoStringTable)?
          .read_string(value)
          .map_err(|error| ElementError::StringReadError { error })?;

        Ok(Some(BymlElement::<O>::String(string)))
      }
      DataType::Array => {
        let container_header =
          ContainerHeader::<O>::read_from_bytes(read_from_pointer(4)?).unwrap();

        let (data_types, values) =
          BymlArrayIter::get_components(self.data, container_header.entries(), value as usize)
            .map_err(|error| ElementError::Array { error })?;

        Ok(Some(BymlElement::Array(BymlArrayIter {
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

        let (entries, _) = BymlDictIter::<O>::get_components(
          self.data,
          container_header.entries(),
          value as usize,
          Some(&self.hash_key_table),
        )
        .map_err(|error| ElementError::Array { error })?;

        Ok(Some(BymlElement::Dict(BymlDictIter {
          data: self.data,
          string_table: self.string_table,
          hash_key_table: self.hash_key_table,
          entries,
          _p: PhantomData,
        })))
      }
      DataType::StringTable => Err(ElementError::UnexpectedStringTable),
      DataType::Bool => Ok(Some(BymlElement::<O>::Bool(value > 0))),
      DataType::I32 => Ok(Some(BymlElement::<O>::I32(i32::from_ne_bytes(
        value.to_ne_bytes(),
      )))),
      DataType::F32 => Ok(Some(BymlElement::<O>::F32(f32::from_ne_bytes(
        value.to_ne_bytes(),
      )))),
      DataType::U32 => Ok(Some(BymlElement::<O>::U32(u32::from_ne_bytes(
        value.to_ne_bytes(),
      )))),
      DataType::I64 => {
        let value = read_from_pointer(8)?;
        let value = I64::<O>::read_from_bytes(value).unwrap();

        Ok(Some(BymlElement::<O>::I64(value.get())))
      }
      DataType::U64 => {
        let value = read_from_pointer(8)?;
        let value = U64::<O>::read_from_bytes(value).unwrap();

        Ok(Some(BymlElement::<O>::U64(value.get())))
      }
      DataType::F64 => {
        let value = read_from_pointer(8)?;
        let value = F64::<O>::read_from_bytes(value).unwrap();

        Ok(Some(BymlElement::<O>::F64(value.get())))
      }
      DataType::Null => Ok(Some(BymlElement::Null)),
    }
  }

  pub fn keys(&self) -> impl Iterator<Item = Result<&CStr, StringReadError>> {
    self
      .entries
      .iter()
      .map(|entry| self.hash_key_table.read_string(entry.hash_key_index()))
  }

  pub fn entries(&self) -> impl Iterator<Item = Result<(&CStr, BymlElement<'_, O>), ElementError>> {
    (0..self.entries.len()).map(|index| -> Result<_, ElementError> {
      let string = self
        .hash_key_table
        .read_string(self.entries[index].hash_key_index())
        .map_err(|error| ElementError::HashKeyReadError { error })?;
      Ok((string, self.get_element(string.to_bytes())?.unwrap()))
    })
  }
}

impl<'a, O: ByteOrder> Debug for BymlDictIter<'a, O> {
  fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
    self.entries().collect::<Result<Vec<_>, _>>().fmt(f)
  }
}

#[derive(Debug)]
pub enum BymlElement<'a, O: ByteOrder> {
  Array(BymlArrayIter<'a, O>),
  Dict(BymlDictIter<'a, O>),
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
