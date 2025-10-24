use std::{
  collections::{BTreeMap, HashMap, HashSet},
  ffi::CString,
  hash::{BuildHasherDefault, DefaultHasher},
  io::{self, Seek, SeekFrom, Write},
  ops::{Deref, DerefMut},
  rc::Rc,
};

use either::Either;
use ordered_float::OrderedFloat;
use zerocopy::{ByteOrder, F64, I64, Immutable, IntoBytes, U16, U32, U64};

use crate::{
  byml::{
    types::{ContainerHeader, DataType, DictEntry, Header},
    write_error::{Overflowed, WriteError},
  },
  util::align_up,
};

#[derive(Hash, PartialEq, Eq)]
pub struct BymlWriterArray {
  elements: Vec<BymlWriterNode>,
}

impl BymlWriterArray {
  pub fn new() -> Self {
    Self {
      elements: Vec::new(),
    }
  }

  pub fn push_string<A: AsRef<str>>(&mut self, value: A) {
    self.elements.push(BymlWriterNode::String(
      CString::new(value.as_ref()).expect("failed to convert value to cstring"),
    ))
  }

  pub fn push_null(&mut self) {
    self.elements.push(BymlWriterNode::Null);
  }

  fn inline_size<O: ByteOrder>(&self) -> Option<u32> {
    size_of::<ContainerHeader<O>>()
      .checked_add(align_up(self.len(), 4))?
      .checked_add(align_up(self.len() * 4, 4))?
      .try_into()
      .ok()
  }
}

macro_rules! array_push_impl {
  ($(($func: ident, $value: ty, $variant: ident)),*) => {
    impl BymlWriterArray {
      $(
        #[allow(unused_parens)]
        pub fn $func(&mut self, value: $value) {
          self.elements.push(BymlWriterNode::$variant(value.into()));
        }
      )*
    }
  };
}

array_push_impl! {
  (push_array, (impl Into<Rc<BymlWriterArray>>), Array),
  (push_dict, (impl Into<Rc<BymlWriterDict>>), Dictionary),
  (push_bool, bool, Bool),
  (push_i32, i32, I32),
  (push_u32, u32, U32),
  (push_f32, f32, F32),
  (push_i64, i64, I64),
  (push_u64, u64, U64),
  (push_f64, f64, F64)
}

impl Deref for BymlWriterArray {
  type Target = Vec<BymlWriterNode>;

  fn deref(&self) -> &Self::Target {
    &self.elements
  }
}

impl DerefMut for BymlWriterArray {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.elements
  }
}

#[derive(Hash, PartialEq, Eq)]
pub struct BymlWriterDict {
  entries: BTreeMap<CString, BymlWriterNode>,
}

impl Deref for BymlWriterDict {
  type Target = BTreeMap<CString, BymlWriterNode>;

  fn deref(&self) -> &Self::Target {
    &self.entries
  }
}

impl DerefMut for BymlWriterDict {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.entries
  }
}

impl BymlWriterDict {
  pub fn new() -> Self {
    Self {
      entries: BTreeMap::new(),
    }
  }

  pub fn insert_string(&mut self, key: impl AsRef<str>, value: impl AsRef<str>) {
    self.entries.insert(
      CString::new(key.as_ref()).expect("failed to convert key to cstring"),
      BymlWriterNode::String(
        CString::new(value.as_ref()).expect("failed to convert value to cstring"),
      ),
    );
  }

  pub fn insert_null(&mut self, key: impl AsRef<str>) {
    self.entries.insert(
      CString::new(key.as_ref()).expect("failed to convert key to cstring"),
      BymlWriterNode::Null,
    );
  }

  fn inline_size<O: ByteOrder>(&self) -> Option<u32> {
    size_of::<ContainerHeader<O>>()
      .checked_add(align_up(self.len() * size_of::<DictEntry<O>>(), 4))?
      .try_into()
      .ok()
  }
}

macro_rules! dict_push_impl {
  ($(($func: ident, $value: ty, $variant: ident)),*) => {
    impl BymlWriterDict {
      $(
        #[allow(unused_parens)]
        pub fn $func(&mut self, key: &str, value: $value) {
          self.entries.insert(
            CString::new(key).expect("failed to convert key to cstring"),
            BymlWriterNode::$variant(value.into())
          );
        }
      )*
    }
  };
}

dict_push_impl! {
  (insert_array, (impl Into<Rc<BymlWriterArray>>), Array),
  (insert_dict, (impl Into<Rc<BymlWriterDict>>), Dictionary),
  (insert_bool, bool, Bool),
  (insert_i32, i32, I32),
  (insert_u32, u32, U32),
  (insert_f32, f32, F32),
  (insert_i64, i64, I64),
  (insert_u64, u64, U64),
  (insert_f64, f64, F64)
}

#[derive(Hash, PartialEq, Eq)]
pub enum BymlWriterNode {
  Array(Rc<BymlWriterArray>),
  Dictionary(Rc<BymlWriterDict>),
  Bool(bool),
  I32(i32),
  F32(OrderedFloat<f32>),
  U32(u32),
  I64(i64),
  U64(u64),
  F64(OrderedFloat<f64>),
  String(CString),
  Null,
}

impl BymlWriterNode {
  fn data_type(&self) -> DataType {
    match self {
      BymlWriterNode::Array(_) => DataType::Array,
      BymlWriterNode::Dictionary(_) => DataType::Dictionary,
      BymlWriterNode::Bool(_) => DataType::Bool,
      BymlWriterNode::I32(_) => DataType::I32,
      BymlWriterNode::F32(_) => DataType::F32,
      BymlWriterNode::U32(_) => DataType::U32,
      BymlWriterNode::I64(_) => DataType::I64,
      BymlWriterNode::U64(_) => DataType::U64,
      BymlWriterNode::F64(_) => DataType::F64,
      BymlWriterNode::String(_) => DataType::String,
      BymlWriterNode::Null => DataType::Null,
    }
  }
}

#[derive(Clone, Hash, PartialEq, Eq)]
enum Container {
  Array(Rc<BymlWriterArray>),
  Dictionary(Rc<BymlWriterDict>),
}

impl Container {
  fn inline_size<O: ByteOrder>(&self) -> Option<u32> {
    match self {
      Container::Array(byml_writer_array) => byml_writer_array.inline_size::<O>(),
      Container::Dictionary(byml_writer_dict) => byml_writer_dict.inline_size::<O>(),
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
  V2,
  V3,
}

type HashState = BuildHasherDefault<DefaultHasher>;

pub struct BymlWriter {
  container: Container,
  containers: HashSet<Container, HashState>,
}

impl BymlWriter {
  pub fn from_array(array: impl Into<Rc<BymlWriterArray>>) -> Self {
    Self::new(Container::Array(array.into()))
  }

  pub fn from_dictionary(dict: impl Into<Rc<BymlWriterDict>>) -> Self {
    Self::new(Container::Dictionary(dict.into()))
  }

  fn new(container: Container) -> Self {
    assert!(size_of::<usize>() >= 4, "cannot be executed on 16 bit platforms");
    let mut containers = HashSet::default();

    let mut stack = Vec::new();
    stack.push(container.clone());

    while let Some(container) = stack.pop() {
      containers.insert(container.clone());

      match container {
        Container::Array(array) => stack.extend(array.iter().filter_map(|f| match f {
          BymlWriterNode::Array(array) => Some(Container::Array(array.clone())),
          BymlWriterNode::Dictionary(dict) => Some(Container::Dictionary(dict.clone())),
          _ => None,
        })),
        Container::Dictionary(array) => stack.extend(array.iter().filter_map(|(_, f)| match f {
          BymlWriterNode::Array(array) => Some(Container::Array(array.clone())),
          BymlWriterNode::Dictionary(dict) => Some(Container::Dictionary(dict.clone())),
          _ => None,
        })),
      }
    }

    Self {
      container,
      containers,
    }
  }

  fn traverse_containers<'a>(
    &'a self,
    mut func: impl FnMut(&'a Container) -> Result<(), WriteError>,
  ) -> Result<(), WriteError> {
    for ele in &self.containers {
      func(ele)?;
    }

    Ok(())
  }

  // todo: panic handling for arithmetic
  pub fn write<O: ByteOrder>(
    &self,
    writer: &mut (impl Write + Seek),
    version: Version,
  ) -> Result<(), WriteError> {
    let mut strings: HashSet<&CString, HashState> = HashSet::default();
    let mut keys: HashSet<&CString, HashState> = HashSet::default();
    let mut data_size = 0u32;
    let mut container_offset = 0u32;
    let mut containers: HashMap<&Container, u32, HashState> = HashMap::default();

    self.traverse_containers(|cont| {
      containers.insert(cont, container_offset);
      let inline_size = align_up(cont.inline_size::<O>().ok_or(Overflowed)?, 4);
      container_offset = container_offset
        .checked_add(inline_size as u32)
        .ok_or(Overflowed)?;
      data_size = data_size.checked_add(inline_size).ok_or(Overflowed)?;
      let iter = match cont {
        Container::Array(array) => {
          Either::Left(array.iter().map(|value| (None::<&CString>, value)))
        }
        Container::Dictionary(dict) => {
          Either::Right(dict.iter().map(|(key, value)| (Some(key), value)))
        }
      };

      for (key, value) in iter {
        if let Some(key) = key {
          keys.insert(key);
        }
        match value {
          BymlWriterNode::String(string) => {
            strings.insert(string);
          }
          BymlWriterNode::I64(_) | BymlWriterNode::U64(_) | BymlWriterNode::F64(_) => {
            data_size = data_size.checked_add(8).ok_or(Overflowed)?;
          }
          _ => {}
        }
      }

      Ok(())
    })?;

    let strings_len = align_up(
      strings
        .iter()
        .map(|string| string.as_bytes_with_nul().len())
        .try_fold(0usize, |a, b| a.checked_add(b))
        .ok_or(Overflowed)?,
      4,
    );
    let keys_len = align_up(
      keys
        .iter()
        .map(|key| key.as_bytes_with_nul().len())
        .try_fold(0usize, |a, b| a.checked_add(b))
        .ok_or(Overflowed)?,
      4,
    );

    let calc_table_total = |table: &HashSet<_, _>, table_len: u32| -> Result<u32, Overflowed> {
      Ok(align_up(
        (size_of::<ContainerHeader<O>>() as u32)
          .checked_add(align_up(
            u32::try_from(table.len())
              .map_err(|_| Overflowed)?
              .checked_add(1)
              .ok_or(Overflowed)?
              .checked_mul(4)
              .ok_or(Overflowed)?,
            4,
          ))
          .ok_or(Overflowed)?
          .checked_add(table_len)
          .ok_or(Overflowed)?,
        4,
      ))
    };
    let hash_key_offset = size_of::<Header<O>>() as u32;
    let keys_total = if keys.is_empty() {
      0
    } else {
      calc_table_total(&keys, u32::try_from(keys_len).map_err(|_| Overflowed)?)?
    };
    let string_table_offset = hash_key_offset + keys_total;
    let strings_total = if strings.is_empty() {
      0
    } else {
      calc_table_total(
        &strings,
        u32::try_from(strings_len).map_err(|_| Overflowed)?,
      )?
    };

    let nodes_start_offset: u32 = align_up(
      string_table_offset
        .checked_add(strings_total)
        .ok_or(Overflowed)?,
      4,
    )
    .try_into()
    .map_err(|_| Overflowed)?;

    let header = Header::<O> {
      magic: match O::ORDER {
        zerocopy::Order::BigEndian => *b"BY",
        zerocopy::Order::LittleEndian => *b"YB",
      },
      version: U16::<O>::new(match version {
        Version::V2 => 2,
        Version::V3 => 3,
      }),
      hash_key_offset: U32::<O>::new(hash_key_offset as _),
      string_table_offset: U32::<O>::new(string_table_offset as _),
      root_node_offset: U32::<O>::new(
        (nodes_start_offset as u32)
          .checked_add(*containers.get(&self.container).unwrap())
          .ok_or(Overflowed)?,
      ),
    };

    writer.write_all(header.as_bytes())?;
    let keys = Self::write_string_table::<O>(keys, writer)?;
    writer.seek(SeekFrom::Start(string_table_offset as u64))?;
    let strings = Self::write_string_table::<O>(strings, writer)?;
    writer.seek(SeekFrom::Start(nodes_start_offset as u64))?;

    let mut long_offset = nodes_start_offset
      .checked_add(container_offset)
      .ok_or(Overflowed)?;
    let mut element_types: Vec<DataType> = Vec::new();
    let mut element_values: Vec<u32> = Vec::new();
    for cont in &self.containers {
      let container_offset = containers
        .get(&cont)
        .expect("missed reference during container ingest");
      writer.seek(io::SeekFrom::Start(
        nodes_start_offset
          .checked_add(*container_offset)
          .ok_or(Overflowed)? as u64,
      ))?;

      match cont {
        Container::Array(array) => {
          let header =
            ContainerHeader::<O>::new(DataType::Array, array.len() as u32).ok_or(Overflowed)?;
          element_types.clear();
          element_values.clear();

          for element in array.iter() {
            element_types.push(element.data_type());
            element_values.push(Self::get_value::<O>(
              &containers,
              writer,
              nodes_start_offset as u32,
              &mut long_offset,
              &strings,
              element,
            )?);
          }

          writer.write_all(header.as_bytes())?;
          writer.write_all(element_types.as_bytes())?;
          let align = 4 - (writer.stream_position()?.cast_signed() & 3);
          writer.seek_relative(align)?;
          writer.write_all(element_values.as_bytes())?;
        }
        Container::Dictionary(dict) => {
          let header =
            ContainerHeader::<O>::new(DataType::Dictionary, dict.len() as u32).ok_or(Overflowed)?;
          writer.write_all(header.as_bytes())?;
          for (key, element) in dict.iter() {
            let key = keys.get(key).expect("missed key in key ingest");
            let value = Self::get_value::<O>(
              &containers,
              writer,
              nodes_start_offset as u32,
              &mut long_offset,
              &strings,
              &element,
            )?;
            writer.write_all(
              DictEntry::<O>::new(element.data_type(), *key, value)
                .ok_or(Overflowed)?
                .as_bytes(),
            )?;
          }
        }
      }
    }

    writer.flush()?;

    Ok(())
  }

  fn write_string_table<'a, O: ByteOrder>(
    table: HashSet<&'a CString, HashState>,
    writer: &mut (impl Write + Seek),
  ) -> Result<BTreeMap<&'a CString, u32>, WriteError> {
    let mut offset = size_of::<ContainerHeader<O>>() + align_up((table.len() + 1) * 4, 4);
    let mut offsets = Vec::with_capacity(align_up(table.len() + 1, 4));
    let mut table = table.into_iter().collect::<Vec<_>>();
    table.sort();

    let table = table
      .into_iter()
      .enumerate()
      .map(|(index, value)| {
        let current_offset = offset;
        offsets.push(current_offset as u32);
        offset += value.as_bytes_with_nul().len();
        (value, index as _)
      })
      .collect::<BTreeMap<_, _>>();
    offsets.push((offset - 1) as u32);

    let header =
      ContainerHeader::<O>::new(DataType::StringTable, table.len() as u32).ok_or(Overflowed)?;
    writer.write_all(header.as_bytes())?;
    writer.write_all(offsets.as_bytes())?;
    for (key, _index) in &table {
      writer.write_all(key.as_bytes_with_nul())?;
    }

    Ok(table)
  }

  fn write_long<T: IntoBytes + Immutable>(
    writer: &mut (impl Write + Seek),
    long_offset: &mut u32,
    value: T,
  ) -> Result<u32, WriteError> {
    let position = writer.stream_position()?;
    let offset = writer.seek(SeekFrom::Start(*long_offset as u64))?;
    *long_offset = long_offset
      .checked_add(value.as_bytes().len().try_into().map_err(|_| Overflowed)?)
      .ok_or(Overflowed)?;
    writer.write_all(value.as_bytes())?;
    writer.seek(SeekFrom::Start(position))?;
    Ok(offset as u32)
  }

  fn get_value<O: ByteOrder>(
    containers: &HashMap<&Container, u32, HashState>,
    writer: &mut (impl Write + Seek),
    nodes_start_offset: u32,
    long_offset: &mut u32,
    strings: &BTreeMap<&CString, u32>,
    ele: &BymlWriterNode,
  ) -> Result<u32, WriteError> {
    let value = match ele {
      BymlWriterNode::Array(array) => nodes_start_offset
        .checked_add(
          *containers
            .get(&Container::Array(array.clone()))
            .expect("missed reference during container ingest"),
        )
        .ok_or(Overflowed)?,
      BymlWriterNode::Dictionary(dict) => nodes_start_offset
        .checked_add(
          *containers
            .get(&Container::Dictionary(dict.clone()))
            .expect("missed reference during container ingest"),
        )
        .ok_or(Overflowed)?,
      BymlWriterNode::Bool(value) => {
        if *value {
          1
        } else {
          0
        }
      }
      BymlWriterNode::I32(value) => value.cast_unsigned(),
      BymlWriterNode::F32(value) => u32::from_ne_bytes(value.to_ne_bytes()),
      BymlWriterNode::U32(value) => *value,
      BymlWriterNode::I64(value) => {
        Self::write_long::<I64<O>>(writer, long_offset, I64::new(*value))?
      }
      BymlWriterNode::U64(value) => {
        Self::write_long::<U64<O>>(writer, long_offset, U64::new(*value))?
      }
      BymlWriterNode::F64(value) => {
        Self::write_long::<F64<O>>(writer, long_offset, F64::new(**value))?
      }
      BymlWriterNode::String(cstring) => *strings
        .get(cstring)
        .expect("missed string during string ingest"),
      BymlWriterNode::Null => 0,
    };

    Ok(value)
  }
}
