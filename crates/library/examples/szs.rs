use std::{
  fs::{self},
  io::Cursor,
};

use senobi_library::{
  byml::reader::BymlReader, sarc::{self, reader::SarcReader}, yaz0::{self, DecompressionError}
};
use zerocopy::LittleEndian;

#[snafu::report]
fn main() -> Result<(), DecompressionError> {
  let slice = include_bytes!("./Bed.szs").as_slice();
  let sarc = yaz0::decompress(&mut Cursor::new(slice))?;
  fs::write("target/Bed.sarc", &sarc)?;
  let reader =SarcReader::<LittleEndian>::new(&sarc).unwrap();
  reader.entries().for_each(|(index, data)| {
    println!("{index:?}, {}", data.len());
  });
  
  let reader = BymlReader::<LittleEndian>::new(reader.get("Bed.byml").expect("what")).expect("not located properly");
  
  for ele in reader.unwrap_dictionary().cstr_entries() {
    
    println!("{ele:?}");
  }
  Ok(())
}
