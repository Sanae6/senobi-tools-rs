use std::{
  fs::{File, read},
  io::{Cursor, Write},
};

use ddsfile::{Caps2, Dds, DxgiFormat, Header, NewDxgiParams};
use senobi_library::nw::{
  bntx::reader::BntxReader,
  gfx::{ChannelFormat, FormatInfo, TypeFormat},
};
use snafu::ErrorCompat;
use tegra_swizzle::surface::deswizzle_surface;
use zerocopy::LittleEndian;

fn main() {
  let file_data = include_bytes!("HomeBed.bntx");
  match BntxReader::<LittleEndian>::read(file_data) {
    Ok(bntx) => {
      for (name, texture) in bntx.textures {
        let params = NewDxgiParams {
          width: texture.width(),
          height: texture.height(),
          depth: None,
          format: match texture.image_format() {
            (ChannelFormat::BC1, TypeFormat::SRGB) => DxgiFormat::BC1_UNorm_sRGB,
            (ChannelFormat::BC4, TypeFormat::Unorm) => DxgiFormat::BC4_UNorm,
            (ChannelFormat::BC5, TypeFormat::Snorm) => DxgiFormat::BC5_SNorm,
            (channel_format, type_format) => todo!("{channel_format:?} {type_format:?}"),
          },
          mipmap_levels: Some(texture.info.info.mip_levels.get() as u32),
          array_layers: Some(texture.info.info.array_layers.get()),
          caps2: None,
          is_cubemap: false,
          resource_dimension: ddsfile::D3D10ResourceDimension::Texture2D,
          alpha_mode: ddsfile::AlphaMode::Unknown,
        };
        let mut dxgi = Dds::new_dxgi(params).unwrap();
        dxgi.data = texture.deswizzled_image_data().unwrap();
        dxgi.write(&mut File::create(format!("target/{name}.dds")).unwrap()).unwrap();
      }

      let data = read(format!("target/BedBody_alb.dds")).unwrap();
      let body = image::load(&mut Cursor::new(&data), image::ImageFormat::Dds).unwrap();
      body.save("target/BedBody_alb.png").unwrap();
    }
    Err(error) => {
      for ele in error.iter_chain() {
        println!("{}", ele);
      }
      if let Some(backtrace) = error.backtrace() {
        println!("{:?}", backtrace)
      }
    }
  }
}
