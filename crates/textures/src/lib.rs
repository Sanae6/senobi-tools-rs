pub mod formats;

pub trait TextureReader {
  type Pixel;
  type Error;

  fn width(&self) -> u32;
  fn height(&self) -> u32;
  
  fn decompress(&self) -> Result<Vec<u8>, Self::Error>;
}
