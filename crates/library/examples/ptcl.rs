use std::{fs, io::Cursor};

use senobi_library::{
  sarc::{self, reader::SarcReader},
  yaz0,
};
use zerocopy::LittleEndian;

fn main() {
  let whopper = fs::read("../Switch/odyssey/romfs/EffectData/EffectPtcl.szs").unwrap();
  let whopper = yaz0::decompress(&mut Cursor::new(whopper)).unwrap();
  let sarc = SarcReader::<LittleEndian>::new(&whopper).unwrap();

  for (key, value) in sarc.entries() {
    println!("{key:?}");
    fs::write(format!("target/{key:?}"), value).unwrap();
  }
}
