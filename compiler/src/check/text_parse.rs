//! 文字列からスカラー値への定数変換を、ランタイムと共通のCパーサーで評価する。
//!
//! 利用元: [`crate::check::constants`]。

use super::ir::{CheckedExpression, Value};
use crate::ast::{FloatType, IntegerType, Type};

// コンパイラと生成プログラムで、メモリ確保をしない同じパーサーを使う。
pub(super) fn parse(text: &str, target: Type, wrap: bool) -> Option<CheckedExpression> {
    unsafe extern "C" {
        fn suz_parse_integer(
            bytes: *const u8,
            length: u64,
            bits: u32,
            signed_target: bool,
            wrap: bool,
            output: *mut u64,
        ) -> bool;
        fn suz_parse_float_bits(
            bytes: *const u8,
            length: u64,
            width: u32,
            output: *mut u64,
        ) -> bool;
        fn suz_parse_bool(bytes: *const u8, length: u64, output: *mut bool) -> bool;
    }
    let value = match target {
        Type::Char => {
            let mut chars = text.chars();
            let value = chars.next()? as i128;
            if chars.next().is_some() {
                return None;
            }
            Value::Integer { value, minus: None }
        }
        Type::Integer(ty) => {
            let mut bits = 0_u64;
            let signed = ty.bounds().0 < 0;
            // 呼び出しが終わるまで、バイト列と出力先は生存する。
            if !unsafe {
                suz_parse_integer(
                    text.as_ptr(),
                    text.len() as u64,
                    ty.bits(),
                    signed,
                    wrap,
                    &mut bits,
                )
            } {
                return None;
            }
            integer_value(bits, ty)
        }
        Type::Float(ty) => {
            let width = match ty {
                FloatType::F32 => 32,
                FloatType::F64 => 64,
            };
            let mut bits = 0_u64;
            // パーサーは生存中のバイト列を読み、一つのu64を書き込む。
            if !unsafe { suz_parse_float_bits(text.as_ptr(), text.len() as u64, width, &mut bits) }
            {
                return None;
            }
            Value::Float(bits)
        }
        Type::Bool => {
            let mut value = false;
            // パーサーはtext.len()バイトだけを読み、一つのboolを書き込む。
            if !unsafe { suz_parse_bool(text.as_ptr(), text.len() as u64, &mut value) } {
                return None;
            }
            Value::Bool(value)
        }
        _ => return None,
    };
    Some(CheckedExpression { ty: target, value })
}

#[cfg(test)]
#[path = "text_parse_tests.rs"]
mod tests;

pub(super) fn integer_value(bits: u64, ty: IntegerType) -> Value {
    let mut value = i128::from(bits);
    if ty.bounds().0 < 0 && bits & (1_u64 << (ty.bits() - 1)) != 0 {
        value -= 1_i128 << ty.bits();
    }
    Value::Integer { value, minus: None }
}
