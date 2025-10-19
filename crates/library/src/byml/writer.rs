use std::{
  collections::{BTreeMap, HashMap, HashSet, btree_map::Entry},
  ffi::CString,
  io::{self, Seek, SeekFrom, Write},
  marker::PhantomData,
  ops::{Deref, DerefMut},
  rc::Rc,
};

use either::Either;
use ordered_float::OrderedFloat;
use zerocopy::{ByteOrder, F64, I16, I64, Immutable, IntoByteSlice, IntoBytes, U16, U32, U64};

use crate::{
  byml::types::{ContainerHeader, DataType, DictEntry, Header},
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

  fn inline_size<O: ByteOrder>(&self) -> usize {
    size_of::<ContainerHeader<O>>() + align_up(self.len(), 4) + align_up(self.len() * 4, 4)
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

  fn inline_size<O: ByteOrder>(&self) -> usize {
    size_of::<ContainerHeader<O>>() + align_up(self.len() * 8, 4)
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
  fn inline_size<O: ByteOrder>(&self) -> usize {
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

pub struct BymlWriter {
  container: Container,
  containers: HashSet<Container>,
}

impl BymlWriter {
  pub fn from_array(array: impl Into<Rc<BymlWriterArray>>) -> Self {
    Self::new(Container::Array(array.into()))
  }

  pub fn from_dictionary(dict: impl Into<Rc<BymlWriterDict>>) -> Self {
    Self::new(Container::Dictionary(dict.into()))
  }

  fn new(container: Container) -> Self {
    let mut containers = HashSet::new();
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

  fn traverse_containers<'a>(&'a self, mut func: impl FnMut(&'a Container)) {
    for ele in &self.containers {
      func(ele);
    }
  }

  // todo: panic handling for arithmetic
  pub fn write<O: ByteOrder>(&self, mut writer: impl Write + Seek, version: Version) -> io::Result<()> {
    let mut strings: HashSet<&CString> = HashSet::new();
    let mut keys: HashSet<&CString> = HashSet::new();
    let mut data_size = 0;
    let mut container_offset = 0;
    let mut containers: HashMap<&Container, u32> = HashMap::new();
    
    self.traverse_containers(|cont| {
      containers.insert(cont, container_offset);
      let inline_size = align_up(cont.inline_size::<O>(), 4);
      container_offset += inline_size as u32;
      data_size += inline_size;
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
            data_size += 8;
          }
          _ => {}
        }
      }
    });

    let strings_len = align_up(
      strings
        .iter()
        .map(|string| string.as_bytes_with_nul().len())
        .sum(),
      4,
    );
    let keys_len = align_up(
      keys.iter().map(|key| key.as_bytes_with_nul().len()).sum(),
      4,
    );

    let hash_key_offset = size_of::<Header<O>>();
    let keys_total = if keys.is_empty() {
      0
    } else {
      align_up(size_of::<ContainerHeader<O>>() + align_up((keys.len() + 1) * 4, 4) + keys_len, 4)
    };
    let string_table_offset = hash_key_offset + keys_total;
    let strings_total = if strings.is_empty() {
      0
    } else {
      align_up(size_of::<ContainerHeader<O>>() + align_up((strings.len() + 1) * 4, 4) + strings_len, 4)
    };

    let root_node_offset = align_up(string_table_offset + strings_total, 4);

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
      root_node_offset: U32::<O>::new(root_node_offset as _),
    };

    fn write_string_table<'a, O: ByteOrder>(
      table: HashSet<&'a CString>,
      writer: &mut (impl Write + Seek),
    ) -> Result<BTreeMap<&'a CString, u32>, io::Error> {
      let mut offset = size_of::<ContainerHeader<O>>() + align_up((table.len() + 1) * 4, 4);
      let mut offsets = Vec::with_capacity(align_up(table.len() + 1, 4));

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

      let header = ContainerHeader::<O>::new(DataType::StringTable, table.len() as u32);
      writer.write_all(header.as_bytes())?;
      writer.write_all(offsets.as_bytes())?;
      for (key, _index) in &table {
        writer.write_all(key.as_bytes_with_nul())?;
      }

      Ok(table)
    }

    writer.write_all(header.as_bytes())?;
    let keys = write_string_table::<O>(keys, &mut writer)?;
    writer.seek(SeekFrom::Start(string_table_offset as u64))?;
    let strings = write_string_table::<O>(strings, &mut writer)?;
    writer.seek(SeekFrom::Start(root_node_offset as u64))?;

    let mut long_offset = root_node_offset as u32 + container_offset;
    let mut element_types: Vec<DataType> = Vec::new();
    let mut element_values: Vec<u32> = Vec::new();
    for cont in &self.containers {
      let container_offset = containers
        .get(&cont)
        .expect("missed reference during container ingest");
      writer.seek(io::SeekFrom::Start(root_node_offset as u64 + *container_offset as u64))?;

      fn write_long<T: IntoBytes + Immutable>(
        writer: &mut (impl Write + Seek),
        long_offset: &mut u32,
        value: T,
      ) -> io::Result<u32> {
        let position = writer.stream_position()?;
        let offset = writer.seek(SeekFrom::Start(*long_offset as u64))?;
        *long_offset += value.as_bytes().len() as u32;
        writer.write_all(value.as_bytes())?;
        writer.seek(SeekFrom::Start(position))?;
        Ok(offset as u32)
      }

      fn get_value<O: ByteOrder>(
        containers: &HashMap<&Container, u32>,
        writer: &mut (impl Write + Seek),
        long_offset: &mut u32,
        strings: &BTreeMap<&CString, u32>,
        ele: &BymlWriterNode,
      ) -> io::Result<u32> {
        let value = match ele {
          BymlWriterNode::Array(array) => *containers
            .get(&Container::Array(array.clone()))
            .expect("missed reference during container ingest"),
          BymlWriterNode::Dictionary(dict) => *containers
            .get(&Container::Dictionary(dict.clone()))
            .expect("missed reference during container ingest"),
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
            write_long::<I64<O>>(writer, long_offset, I64::new(*value))?
          }
          BymlWriterNode::U64(value) => {
            write_long::<U64<O>>(writer, long_offset, U64::new(*value))?
          }
          BymlWriterNode::F64(value) => {
            write_long::<F64<O>>(writer, long_offset, F64::new(**value))?
          }
          BymlWriterNode::String(cstring) => *strings
            .get(cstring)
            .expect("missed string during string ingest"),
          BymlWriterNode::Null => 0,
        };

        Ok(value)
      }
      match cont {
        Container::Array(array) => {
          let header = ContainerHeader::<O>::new(DataType::Array, array.len() as u32);
          element_types.clear();
          element_values.clear();

          for element in array.iter() {
            element_types.push(element.data_type());
            element_values.push(get_value::<O>(
              &containers,
              &mut writer,
              &mut long_offset,
              &strings,
              element,
            )?);
          }

          writer.write_all(header.as_bytes())?;
          writer.write_all(element_types.as_bytes())?;
          let extra = [0; 3];
          writer.write_all(&extra[(element_types.len() % 4)..])?;
          writer.write_all(element_values.as_bytes())?;
        }
        Container::Dictionary(dict) => {
          let header = ContainerHeader::<O>::new(DataType::Dictionary, dict.len() as u32);
          writer.write_all(header.as_bytes())?;
          for (key, element) in dict.iter() {
            let key = keys.get(key).expect("missed key in key ingest");
            let value = get_value::<O>(
              &containers,
              &mut writer,
              &mut long_offset,
              &strings,
              &element,
            )?;
            writer.write_all(DictEntry::<O>::new(element.data_type(), *key, value).as_bytes())?;
          }
        }
      }
    }
    Ok(())
  }
}
