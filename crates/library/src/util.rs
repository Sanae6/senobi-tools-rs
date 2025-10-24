use num_traits::PrimInt;

use crate::byml::reader::BymlReader;

pub fn align_up<T: PrimInt>(value: T, alignment: T) -> T {
  (value + alignment - T::one()) & !(alignment - T::one())
}

#[derive(Debug)]
pub enum Order {
  LittleEndian,
  BigEndian,
}

pub trait DetectEndianness {
  type Error;
  fn detect() -> Result<Order, Self::Error>;
}

struct DetectEndian;

impl<'a> DetectEndianness for BymlReader<'a, DetectEndian> {
  type Error = ();

  fn detect() -> Result<Order, Self::Error> {
    todo!()
  }
}

macro_rules! read_endian_agnostic {
  () => {
    
  };
}

// fn testing() {
//   let byml: ;
//   match BymlReader::<DetectEndian>::detect().unwrap() {
//     Order::LittleEndian => {
//       |reader| -> Result<(), BymlReader<>
//     },
//     Order::BigEndian => todo!(),
//   }
// }
