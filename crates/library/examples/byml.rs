use std::{fs, io::Cursor};

use senobi_library::byml::{
  self,
  reader::BymlReader,
  writer::{BymlWriter, BymlWriterArray, BymlWriterDict},
};
use zerocopy::LittleEndian;

fn main() {
  let slice = include_bytes!("./Bed.byml");
  let BymlReader::Dictionary(dict) = byml::reader::BymlReader::<LittleEndian>::new(slice).unwrap()
  else {
    panic!()
  };

  let element = dict.get_string("UnitConfigName").unwrap().unwrap();
  println!("{element:?}");
  // println!("{dict:?}");

  let mut array = BymlWriterArray::new();
  array.push_u64(42);

  let mut dict = BymlWriterDict::new();
  dict.insert_string("hello", "world");
  dict.insert_u32("hi", 53);
  dict.insert_f32("hu", 53.4);
  dict.insert_f64("hr", 53.84);
  dict.insert_array("haray", array);
  let dict = BymlWriter::from_dictionary(dict);
  let mut data = Vec::new();
  let mut writer = Cursor::new(&mut data);
  dict
    .write::<LittleEndian>(&mut writer, byml::writer::Version::V3)
    .unwrap();
  fs::write("target/my_awesome.byml", &data).unwrap();

  let BymlReader::Dictionary(dict) = BymlReader::<LittleEndian>::new(&data).unwrap() else {
    panic!()
  };
  println!("{}", dict.get_string("hello").unwrap().unwrap());
  println!("{}", dict.get_u32("hi").unwrap().unwrap());
  println!("{}", dict.get_f32("hu").unwrap().unwrap());
  println!("{}", dict.get_f64("hr").unwrap().unwrap());
  let array = dict.get_array("haray").unwrap().unwrap();
  println!("{}", array.get_u64(0).unwrap().unwrap());
  // println!("{}", array.get_u64(0x1003040).unwrap().unwrap());
}
