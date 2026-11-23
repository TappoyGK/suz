//! テンプレートの要求型とtextbufを検査し、定数文字列を評価する。symbolだけを登録対象にする。
//!
//! 利用元: [`crate::check::expression`]、[`crate::check::statements`]。
//! 主な対象規則: `[symbol.template]`、`[symbol.interpolation]`、`[text.buffer]`。

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
                            | Type::ReadOnlyArray(_)
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

pub(super) fn expression(
    expr: &Expression,
    required: Option<Type>,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    match &expr.kind {
        crate::ast::ExpressionKind::Template(parts) => template(
            parts,
            expr.location,
            required == Some(Type::Symbol),
            false,
            scope,
        ),
        crate::ast::ExpressionKind::TextBuffer { capacity, value } => {
            buffer(capacity.as_deref(), value, expr.location, scope)
        }
        crate::ast::ExpressionKind::TextAppend { left, right } => {
            let left = super::expression::expression(left, None, scope)?;
            if left.ty != Type::Text {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("text.append", "append requires a text destination"),
                ));
            }
            let right = content(right, scope)?;
            Ok(CheckedExpression {
                ty: Type::Text,
                value: Value::TextAppend {
                    left: Box::new(left),
                    right: Box::new(right),
                    location: expr.location,
                },
            })
        }
        _ => unreachable!(),
    }
}

pub(super) fn template(
    input: &[TemplatePart<Expression>],
    location: Location,
    symbol: bool,
    buffer: bool,
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
    if !symbol && !buffer && constant.is_none() {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("symbol.template", "runtime text templates require textbuf"),
        ));
    }
    Ok(CheckedExpression {
        ty: if symbol { Type::Symbol } else { Type::Text },
        value: Value::Template {
            parts,
            location,
            constant,
        },
    })
}

// 直接のテンプレートだけは、実行時の埋め込みを現在の作業領域へ構築できる。
// 既存のtextはその参照範囲をコピーする。容量の評価順はIRにも残す。
pub(super) fn buffer(
    capacity: Option<&Expression>,
    value: &Expression,
    location: Location,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    if scope.arena_depth == 0 {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("text.buffer", "textbuf requires a current arena"),
        ));
    }
    let capacity = capacity
        .map(|v| {
            super::expression::coerced(v, Some(Type::Integer(crate::ast::IntegerType::Len)), scope)
                .map(Box::new)
        })
        .transpose()?;
    let value = content(value, scope)?;
    Ok(CheckedExpression {
        ty: Type::Text,
        value: Value::TextBuffer {
            capacity,
            value: Box::new(value),
            location,
        },
    })
}

fn content(value: &Expression, scope: &Scope) -> Result<CheckedExpression, Diagnostic> {
    let mut value = value;
    while let crate::ast::ExpressionKind::Group(inner) = &value.kind {
        value = inner;
    }
    let value = if let crate::ast::ExpressionKind::Template(parts) = &value.kind {
        template(parts, value.location, false, true, scope)?
    } else {
        super::expression::coerced(value, Some(Type::Text), scope)?
    };
    Ok(value)
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
        } if value.ty == Type::Symbol => {
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
