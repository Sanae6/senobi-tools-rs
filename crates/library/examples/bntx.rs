use senobi_library::nw::bntx::reader::Bntx;
use snafu::ErrorCompat;
use zerocopy::LittleEndian;

fn main() {
  let file_data = include_bytes!("HomeBed.bntx");
  match Bntx::<LittleEndian>::read(file_data) {
    Ok(bntx) => {
      println!("{:#?}", bntx.textures[1].1.info);
    }
    Err(error) => {
      for ele in error.iter_chain() {
        println!("{}", ele);
      }
      if let Some(backtrace) = error.backtrace() {
        println!("{:?}", backtrace)
      }
      // println!("{}", error.backtrace().unwrap())
    }
  }
}
