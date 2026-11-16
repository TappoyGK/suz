// コンパイラ実装に依存しない、Rustから求めた参照用の値域。
pub const INTEGER_BOUNDS: [(&str, i128, i128); 8] = [
    ("i8", i8::MIN as i128, i8::MAX as i128),
    ("i16", i16::MIN as i128, i16::MAX as i128),
    ("i32", i32::MIN as i128, i32::MAX as i128),
    ("i64", i64::MIN as i128, i64::MAX as i128),
    ("u8", u8::MIN as i128, u8::MAX as i128),
    ("u16", u16::MIN as i128, u16::MAX as i128),
    ("u32", u32::MIN as i128, u32::MAX as i128),
    ("u64", u64::MIN as i128, u64::MAX as i128),
];
