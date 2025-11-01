use std::{fs, io::Cursor};

use senobi_library::{nw::bfres::reader::BfresReaderV8, sarc::reader::SarcReader, yaz0};
use zerocopy::LittleEndian;

fn main() {
  let whopper = include_bytes!("HomeBed.szs");
  let whopper = yaz0::decompress(&mut Cursor::new(whopper)).unwrap();
  let sarc = SarcReader::<LittleEndian>::new(&whopper).unwrap();
  let bfres = sarc.get("HomeBed.bfres").unwrap();
  fs::write("crates/library/examples/HomeBed.bfres", bfres).unwrap();
  let _reader = BfresReaderV8::read(bfres).unwrap();
}
