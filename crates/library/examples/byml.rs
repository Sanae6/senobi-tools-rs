use senobi_library::byml::{self, iter::BymlContainer};
use zerocopy::LittleEndian;

fn main() {
  let slice = include_bytes!("./Bed.byml");
  let BymlContainer::Dictionary(dict) =
    byml::iter::BymlContainer::<LittleEndian>::new(slice).unwrap()
  else {
    panic!()
  };

  let element = dict
    .get_element("UnitConfigName".as_bytes())
    .unwrap()
    .unwrap();
  println!("{element:?}");
  println!("{dict:?}");
}
