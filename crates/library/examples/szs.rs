use std::{
  fs::{self},
  io::Cursor,
};

use senobi_library::{
  sarc::{self, reader::SarcReader},
  yaz0::{self, DecompressionError},
};
use zerocopy::LittleEndian;

#[snafu::report]
fn main() -> Result<(), DecompressionError> {
  let slice = include_bytes!("./MiiSystem.szs").as_slice();
  let sarc = yaz0::decompress(&mut Cursor::new(slice))?;
  fs::write("target/MiiSystem.sarc", &sarc)?;
  SarcReader::<LittleEndian>::new(&sarc).unwrap();
  Ok(())
}
