use super::{
    constants,
    expression::expression,
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
                    let result = expression(value, None, bindings)?;
                    if matches!(result.ty, Type::Void | Type::Struct(_)) {
                        return Err(value.location.error(
                            ErrorKind::InvalidSource,
                            "this type cannot be interpolated [text.interpolation]",
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
    let constant = parts
        .iter()
        .map(|part| match part {
            TemplatePart::Text(text) => Some(text.clone()),
            TemplatePart::Value(value) => {
                let evaluated = constants::evaluate(value).ok()??;
                Some(match evaluated.value {
                    Value::Text { text, .. } => text,
                    Value::Bool(value) => value.to_string(),
                    Value::Integer { value, .. } => match evaluated.ty {
                        Type::Enum(ty) => bindings.enums[ty.id].variants[value as usize].clone(),
                        _ => value.to_string(),
                    },
                    Value::Float(bits) => float_format(bits, evaluated.ty),
                    _ => return None,
                })
            }
        })
        .collect::<Option<String>>();
    Ok(CheckedExpression {
        ty: Type::Text,
        value: Value::Template {
            parts,
            location,
            constant,
        },
    })
}

// Use the same bundled Ryu formatter as generated programs. Rust's default
// formatter makes a different choice for some equally short decimal strings.
unsafe extern "C" {
    fn suz_format_f32(value: f32, output: *mut u8) -> usize;
    fn suz_format_f64(value: f64, output: *mut u8) -> usize;
}

fn float_format(bits: u64, ty: Type) -> String {
    let mut output = [0_u8; 32];
    // The C interface writes at most 32 bytes and returns their count. It
    // owns no Rust memory and retains no pointers after this call.
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

// Collect during checking, before constant substitution or code generation can
// discard unused declarations and unselected operands.
pub(super) fn register_constants(value: &CheckedExpression, scope: &Scope) {
    match &value.value {
        Value::Text { text, .. }
        | Value::Template {
            constant: Some(text),
            ..
        } => {
            scope.constant_texts.borrow_mut().insert(text.clone());
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
