//! テンプレートの補間値を検査し、定数文字列を評価・登録する。浮動小数点の表記はランタイムと共通の変換を使う。
//!
//! 利用元: [`crate::check::expression`]、[`crate::check::statements`]。
//! 主な対象規則: `[symbol.interpolation]`。

use super::{
    constants,
    ir::{CheckedExpression, Value},
    scope::Scope,
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, FloatType, Location, TemplatePart, Type},
};

pub(super) fn parts(
    input: &[TemplatePart<Expression>],
    bindings: &Scope,
) -> Result<Vec<TemplatePart<CheckedExpression>>, Diagnostic> {
    input
        .iter()
        .map(|part| {
            Ok(match part {
                TemplatePart::Text(text) => TemplatePart::Text(text.clone()),
                TemplatePart::Value(value) => {
                    let result = super::exact::use_value(value, bindings)?;
                    if matches!(
                        result.ty,
                        Type::StaticFunction(_)
                            | Type::Function(_)
                            | Type::Void
                            | Type::Parameter(_)
                            | Type::Struct(_)
                            | Type::Pointer { .. }
                            | Type::CPointer
                            | Type::CInteger(_)
                    ) {
                        return Err(value.location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!(
                                "symbol.interpolation",
                                "this type cannot be interpolated"
                            ),
                        ));
                    }
                    TemplatePart::Value(result)
                }
            })
        })
        .collect()
}

pub(super) fn template(
    input: &[TemplatePart<Expression>],
    location: Location,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let parts = parts(input, bindings)?;
    let constant = constant_parts(&parts, bindings);
    if constant
        .as_ref()
        .is_some_and(|text| text.chars().count() > i32::MAX as usize)
    {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "rt.len-limit",
                "symbol exceeds the maximum Unicode scalar count"
            ),
        ));
    }
    Ok(CheckedExpression {
        ty: Type::Symbol,
        value: Value::Template {
            parts,
            location,
            constant,
        },
    })
}

pub(super) fn constant_parts(
    parts: &[TemplatePart<CheckedExpression>],
    bindings: &Scope,
) -> Option<String> {
    parts
        .iter()
        .map(|part| match part {
            TemplatePart::Text(text) => Some(text.clone()),
            TemplatePart::Value(value) => {
                let evaluated = constants::evaluate(value).ok()??;
                Some(match evaluated.value {
                    Value::Text { text, .. } => text,
                    Value::Bool(value) => value.to_string(),
                    Value::Integer { value, .. } => match evaluated.ty {
                        Type::Char => char::from_u32(value as u32).unwrap().to_string(),
                        Type::Enum(ty) => bindings.enums[ty.id].variants[value as usize].clone(),
                        _ => value.to_string(),
                    },
                    Value::Exact(ref e) => super::exact::evaluate(e)?.ok()?.to_string(),
                    Value::Float(bits) => float_format(bits, evaluated.ty),
                    _ => return None,
                })
            }
        })
        .collect::<Option<String>>()
}

// 生成プログラムと同じ同梱Ryuフォーマッターを使う。
// Rustの標準フォーマッターは、同じ短さの十進表記から別のものを選ぶ場合がある。
unsafe extern "C" {
    fn suz_format_f32(value: f32, output: *mut u8) -> usize;
    fn suz_format_f64(value: f64, output: *mut u8) -> usize;
}

fn float_format(bits: u64, ty: Type) -> String {
    let mut output = [0_u8; 32];
    // Cのインターフェースは最大32バイトを書き、その長さを返す。
    // Rustのメモリを所有せず、呼び出し後にポインターを保持しない。
    let length = unsafe {
        match ty {
            Type::Float(FloatType::F32) => {
                suz_format_f32(f32::from_bits(bits as u32), output.as_mut_ptr())
            }
            Type::Float(FloatType::F64) => {
                suz_format_f64(f64::from_bits(bits), output.as_mut_ptr())
            }
            _ => unreachable!(),
        }
    };
    String::from_utf8(output[..length].to_vec()).expect("numeric formatting produces ASCII")
}

// 定数置換やC生成で未使用の宣言・非選択のオペランドが消える前に、検査中に収集する。
pub(super) fn register_constants(value: &CheckedExpression, scope: &Scope) {
    match &value.value {
        Value::TextView(inner) => register_constants(inner, scope),
        Value::Text { text, .. } if value.ty == Type::Symbol => {
            scope
                .outputs
                .constant_texts
                .borrow_mut()
                .insert(text.clone());
        }
        Value::Intern {
            constant: Some(text),
            ..
        } => {
            scope
                .outputs
                .constant_texts
                .borrow_mut()
                .insert(text.clone());
        }
        Value::Template {
            constant: Some(text),
            ..
        } => {
            scope
                .outputs
                .constant_texts
                .borrow_mut()
                .insert(text.clone());
        }
        Value::Struct { fields } => {
            for (_, field) in fields {
                register_constants(field, scope);
            }
        }
        Value::CheckedCast {
            constant_struct: Some(value),
            ..
        } => register_constants(value, scope),
        _ => {}
    }
}
