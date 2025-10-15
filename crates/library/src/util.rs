use num_traits::PrimInt;

pub fn align_up<T: PrimInt>(value: T, alignment: T) -> T {
  (value + alignment - T::one()) & !(alignment - T::one())
}
