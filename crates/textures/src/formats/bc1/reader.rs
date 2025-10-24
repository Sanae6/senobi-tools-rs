use std::marker::PhantomData;

use crate::{formats::{Rgb565, Srgb}, TextureReader};

pub struct Bc1<F> {
  data: Vec<u8>,
  width: u32,
  height: u32,
  format: PhantomData<F>,
}

impl<F> Bc1<F> {
  pub fn new(width: u32, height: u32, data: Vec<u8>) -> Self {
    Self {
      width,
      height,
      data,
      format: PhantomData,
    }
  }
}

impl TextureReader for Bc1<Srgb> {
  type Pixel = Rgb565;
  type Error = ();
  fn width(&self) -> u32 {
    self.width
  }
  fn height(&self) -> u32 {
    self.height
  }

  fn decompress(&self) -> Result<Vec<u8>, Self::Error> {
    // let 
    // for block in self.data.chunks(8) {
        
    // }
    
    // Ok(())
    todo!()
  }
}
