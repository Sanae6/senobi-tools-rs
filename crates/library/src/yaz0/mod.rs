#![allow(dead_code)]

use std::{
  backtrace::Backtrace,
  io::{self, Read, Seek},
  iter,
};

use modular_bitfield::bitfield;
use snafu::{GenerateImplicitData, OptionExt, Snafu, ensure};
use zerocopy::{FromBytes, FromZeros, Immutable, IntoBytes, KnownLayout, big_endian::U32};

#[derive(FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
struct Header {
  magic: [u8; 4],
  uncompressed_size: U32,
  _unused: [u8; 8],
}

#[derive(Snafu, Debug)]
pub enum DecompressionError {
  #[snafu(display("error while reading file: {source}"))]
  Io {
    source: io::Error,
    backtrace: Backtrace,
  },
  #[snafu(display("incorrect magic, expected {expected:02X?}, got {actual:02X?}"))]
  IncorrectMagic {
    expected: [u8; 4],
    actual: [u8; 4],
    backtrace: Backtrace,
  },
  #[snafu(display(
    "attempted to copy {copy_count} bytes which would add data past the decompressed buffer's end {decompressed_size}, currently at {current_size}"
  ))]
  CopyingPastEnd {
    copy_count: u8,
    current_size: u32,
    decompressed_size: u32,
    backtrace: Backtrace,
  },
  #[snafu(display(
    "attempted to copy {copy_count} bytes from {lookback_distance} bytes before the stream started"
  ))]
  CopyingFromBeforeStart {
    copy_count: u8,
    lookback_distance: u16,
    backtrace: Backtrace,
  },
}

impl From<io::Error> for DecompressionError {
  #[track_caller]
  fn from(value: io::Error) -> Self {
    Self::Io {
      source: value,
      backtrace: Backtrace::generate(),
    }
  }
}

pub fn decompressed_size(reader: &mut impl Read) -> Result<u32, DecompressionError> {
  let mut header = Header::new_zeroed();
  reader.read_exact(header.as_mut_bytes())?;

  ensure!(
    header.magic == *b"Yaz0",
    IncorrectMagicSnafu {
      expected: *b"Yaz0",
      actual: header.magic,
    }
  );

  Ok(header.uncompressed_size.get())
}

use modular_bitfield::prelude::*;
#[bitfield(bits = 16)]
#[derive(Debug)]
struct ShortCopy {
  lookback_upper: B4,
  copy_count: B4,
  lookback_lower: B8,
}

#[bitfield(bits = 24)]
struct LongCopy {
  lookback_upper: B4,
  zero: B4,
  lookback_lower: B8,
  copy_count: u8,
}

#[derive(Debug)]
enum Group {
  Uncompressed,
  Copy,
}

struct Groups {
  value: u8,
  remaining: u8,
}

impl Groups {
  fn empty() -> Groups {
    Self {
      value: 0,
      remaining: 0,
    }
  }

  fn is_empty(&self) -> bool {
    self.remaining == 0
  }

  fn pop(&mut self) -> Option<Group> {
    if self.is_empty() {
      return None;
    }

    let value = if self.value & 0x80 != 0 {
      Group::Uncompressed
    } else {
      Group::Copy
    };
    self.value <<= 1;
    self.remaining = self.remaining.saturating_sub(1);

    Some(value)
  }

  fn refill_and_pop(&mut self, value: u8) -> Group {
    self.value = value;
    self.remaining = 8;

    self.pop().unwrap()
  }
}

pub fn decompress(reader: &mut (impl Read + Seek)) -> Result<Box<[u8]>, DecompressionError> {
  let decomp_size = decompressed_size(reader)?;
  let mut decomp_data = Vec::with_capacity(decomp_size as _);

  let mut read_buffer = [0u8; 3];
  let mut groups = Groups::empty();
  while decomp_data.len() < decomp_data.capacity() {
    let current_group = if let Some(current_group) = groups.pop() {
      current_group
    } else {
      reader.read_exact(&mut read_buffer[0..=0])?;
      groups.refill_and_pop(read_buffer[0])
    };

    println!("got {current_group:?}");

    match current_group {
      Group::Uncompressed => {
        reader.read_exact(&mut read_buffer[0..=0])?;
        decomp_data.push(read_buffer[0]);
      }
      Group::Copy => {
        reader.read_exact(&mut read_buffer[0..=1])?;

        println!(
          "{:02X} {:02X} {:02X}",
          read_buffer[0],
          read_buffer[1],
          read_buffer[0] & 0xF0
        );
        let (copy_count, lookback_distance) = if read_buffer[0] & 0xF0 == 0 {
          reader.read_exact(&mut read_buffer[2..=2])?;
          let long_copy = LongCopy::from_bytes(read_buffer);
          let lookback_distance =
            (long_copy.lookback_upper() as u16) << 8 | (long_copy.lookback_lower() as u16);
          (long_copy.copy_count() + 0x12, lookback_distance + 1)
        } else {
          let short_copy = ShortCopy::from_bytes([read_buffer[0], read_buffer[1]]);
          let lookback_distance =
            (short_copy.lookback_upper() as u16) << 8 | (short_copy.lookback_lower() as u16);

          (short_copy.copy_count() + 0x02, lookback_distance + 1)
        };

        ensure!(
          decomp_data.len().saturating_add(copy_count as _) <= decomp_data.capacity(),
          CopyingPastEndSnafu {
            copy_count,
            current_size: decomp_data.len() as u32,
            decompressed_size: decomp_size
          }
        );

        let start = decomp_data
          .len()
          .checked_sub(lookback_distance as _)
          .context(CopyingFromBeforeStartSnafu {
            copy_count,
            lookback_distance,
          })?;

        let dest_start = decomp_data.len();
        decomp_data.extend(iter::repeat_n(0, copy_count as _));
        decomp_data.copy_within(start..start + copy_count as usize, dest_start);
      }
    }
  }

  Ok(decomp_data.into_boxed_slice())
}
