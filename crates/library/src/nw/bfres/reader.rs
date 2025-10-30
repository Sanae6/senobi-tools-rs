use std::marker::PhantomData;

use zerocopy::ByteOrder;

pub struct BfresReader<'a, O: ByteOrder> {
  file_data: &'a [u8],
  _phantom: PhantomData<O>
}

impl<'a, O: ByteOrder> BfresReader<'a, O>{

}
