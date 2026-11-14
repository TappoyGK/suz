use super::{ExpressionContext, arithmetic::integer_parts, casts::cast_parts};
use crate::check::{CheckedExpression, CheckedHandler, IntegerOperation, Value};
use std::fmt::Write;

pub(super) fn c_string(text: &str) -> String {
    let mut result = String::from("\"");
    for byte in text.bytes() {
        if (32..=126).contains(&byte) && !matches!(byte, b'"' | b'\\') {
            result.push(char::from(byte));
        } else {
            write!(result, "\\{byte:03o}").unwrap();
        }
    }
    result.push('"');
    result
}

pub(super) fn expression(expr: &CheckedExpression, context: &mut ExpressionContext) -> String {
    match expr.value {
        Value::Text { ref text, location } => super::text::constant(text, location, context),
        Value::Template {
            ref parts,
            location,
            ref constant,
        } => match constant {
            Some(text) => super::text::constant(text, location, context),
            None => super::text::template(parts, location, false, context),
        },
        Value::Struct { ref fields } => {
            let crate::ast::Type::Struct(id) = expr.ty else {
                unreachable!()
            };
            super::structs::literal(id, fields, context)
        }
        Value::Field { ref target, index } => {
            format!("({}).suz_field_{index}", expression(target, context))
        }
        Value::Void => String::new(),
        Value::Call { id, ref arguments } => super::functions::call(id, arguments, context),
        Value::Block {
            ref statements,
            ref value,
            ref captures,
        } => super::blocks::emit(statements, value, captures, context),
        Value::IntegerArithmetic {
            ref operation,
            location,
            ref handler,
        } => {
            let right_index = context.temporaries.len() + 1;
            let (setup, condition, converted) = integer_parts(expr, context);
            let fallback = match handler {
                Some(CheckedHandler::Value(value)) => expression(value, context),
                Some(CheckedHandler::Return(_)) => {
                    unreachable!("failure returns must be lowered to statements")
                }
                None => {
                    use crate::ast::BinaryOperator;
                    let name = match operation {
                        IntegerOperation::Binary { operator, .. } => match operator {
                            BinaryOperator::Add => "addition",
                            BinaryOperator::Subtract => "subtraction",
                            BinaryOperator::Multiply => "multiplication",
                            BinaryOperator::Divide => "division",
                            BinaryOperator::Remainder => "remainder",
                            BinaryOperator::ShiftLeft => "left shift",
                            BinaryOperator::ShiftRight => "right shift",
                            _ => unreachable!(),
                        },
                        IntegerOperation::Negate(_) => "negation",
                        IntegerOperation::NegatedLiteral(_) => {
                            unreachable!("literal negation cannot fail")
                        }
                    };
                    let reason = if matches!(
                        operation,
                        IntegerOperation::Binary {
                            operator: BinaryOperator::Divide | BinaryOperator::Remainder,
                            ..
                        }
                    ) {
                        format!(
                            "suz_cmp_{right_index} == 0 ? \"integer {name} by zero [rt.div-zero]\" : \"integer {name} overflow [rt.div-overflow]\""
                        )
                    } else if let IntegerOperation::Binary { operator, .. } = operation
                        && operator.is_shift()
                    {
                        let crate::ast::Type::Integer(ty) = expr.ty else {
                            unreachable!()
                        };
                        format!(
                            "(uint64_t)suz_cmp_{right_index} >= {} ? \"shift count out of range [rt.shift-count]\" : \"integer {name} overflow [rt.shift-overflow]\"",
                            ty.bits()
                        )
                    } else {
                        c_string(&format!("integer {name} overflow [rt.int-overflow]"))
                    };
                    format!(
                        "(suz_panic({}, {}, {}, {reason}), ({})0)",
                        c_string(&context.source_name),
                        location.line,
                        location.column,
                        expr.ty.c_name()
                    )
                }
            };
            format!("({setup}, {condition} ? {converted} : ({fallback}))")
        }
        Value::CastReturn { .. } => unreachable!("cast returns must be lowered to statements"),
        Value::CheckedCast {
            ref constant_struct,
            location,
            wrap,
            ref operand,
            ref fallback,
            ..
        } => {
            if let Some(value) = constant_struct {
                return expression(value, context);
            }
            let (setup, condition, converted) =
                cast_parts(operand, expr.ty, wrap, location, context);
            let fallback = expression(fallback, context);
            format!("({setup}, {condition} ? {converted} : ({fallback}))")
        }
        Value::Cast(ref inner) => {
            if inner.ty == crate::ast::Type::Float(crate::ast::FloatType::F64)
                && expr.ty == crate::ast::Type::Float(crate::ast::FloatType::F32)
            {
                context.narrow_float = true;
                context
                    .float_types
                    .extend([crate::ast::FloatType::F32, crate::ast::FloatType::F64]);
                return format!("suz_narrow_f32({})", expression(inner, context));
            }
            if let crate::ast::Type::Float(ty) = expr.ty {
                context.float_types.insert(ty);
            }
            format!("({})({})", expr.ty.c_name(), expression(inner, context))
        }

        Value::Negate(ref inner) => format!("(-({}))", expression(inner, context)),
        Value::Float(bits) => {
            let crate::ast::Type::Float(ty) = expr.ty else {
                unreachable!()
            };
            context.float_helpers.insert(ty);
            context.float_types.insert(ty);
            format!("suz_{}(UINT64_C({bits}))", ty.name())
        }
        Value::Match {
            ref target,
            ref arms,
            ..
        } => {
            if arms.len() == 1 {
                return format!(
                    "((void)({}), {})",
                    expression(target, context),
                    expression(&arms[0].1, context)
                );
            }
            let index = context.temporaries.len();
            context.temporaries.push(target.ty);
            let name = format!("suz_cmp_{index}");
            let target = expression(target, context);
            let mut result = expression(&arms.last().unwrap().1, context);
            for (pattern, value) in arms[..arms.len() - 1].iter().rev() {
                result = format!(
                    "({name} == {} ? {} : {result})",
                    expression(pattern.as_ref().unwrap(), context),
                    expression(value, context)
                );
            }
            format!("({name} = {target}, {result})")
        }
        Value::If {
            ref branches,
            ref otherwise,
        } => {
            let mut result = expression(otherwise, context);
            for (condition, value) in branches.iter().rev() {
                result = format!(
                    "({} ? {} : {})",
                    expression(condition, context),
                    expression(value, context),
                    result
                );
            }
            result
        }
        Value::Truncate(ref inner) => {
            let crate::ast::Type::Integer(ty) = expr.ty else {
                unreachable!()
            };
            context.bit_helpers.insert(ty);
            format!(
                "suz_bits_{}((uint64_t)({}))",
                ty.name(),
                expression(inner, context)
            )
        }
        Value::BitNot(ref inner) => {
            let crate::ast::Type::Integer(ty) = expr.ty else {
                unreachable!()
            };
            context.bit_helpers.insert(ty);
            format!(
                "suz_bits_{}(~(uint64_t)({}))",
                ty.name(),
                expression(inner, context)
            )
        }
        Value::Binary {
            operator,
            ref left,
            ref right,
        } => {
            let index = context.temporaries.len();
            context.temporaries.extend([left.ty, right.ty]);
            let left = expression(left, context);
            let right = expression(right, context);
            let result = match expr.ty {
                crate::ast::Type::Integer(ty) => {
                    context.bit_helpers.insert(ty);
                    format!(
                        "suz_bits_{}((uint64_t)suz_cmp_{index} {} (uint64_t)suz_cmp_{})",
                        ty.name(),
                        operator.c_symbol(),
                        index + 1
                    )
                }
                crate::ast::Type::Float(ty) => format!(
                    "({})(suz_cmp_{index} {} suz_cmp_{})",
                    ty.c_name(),
                    operator.c_symbol(),
                    index + 1
                ),
                _ => unreachable!(),
            };
            format!(
                "(suz_cmp_{index} = {left}, suz_cmp_{} = {right}, {result})",
                index + 1
            )
        }
        Value::Bool(value) => if value { "1" } else { "0" }.into(),
        Value::Not(ref inner) => format!("(!({}))", expression(inner, context)),
        Value::Logical {
            operator,
            ref left,
            ref right,
        } => format!(
            "({} {} {})",
            expression(left, context),
            operator.symbol(),
            expression(right, context)
        ),
        Value::Comparison {
            ref operators,
            ref operands,
        } => {
            use crate::ast::Type;
            let unsigned = operands[0]
                .ty
                .integer_storage()
                .is_some_and(|ty| ty.bounds().0 == 0);
            let text = operands[0].ty == Type::Text;
            let floating = matches!(operands[0].ty, Type::Float(_));
            let structure = if let Type::Struct(id) = operands[0].ty {
                context.struct_equals.insert(id);
                Some(id)
            } else {
                None
            };
            if !floating && !text && structure.is_none() {
                context.helpers.insert(unsigned);
            }
            // Declarations have no evaluation effects. Assignments remain in
            // the expression so logical short circuit and loop reevaluation hold.
            let names: Vec<_> = operands
                .iter()
                .map(|operand| {
                    let index = context.temporaries.len();
                    context.temporaries.push(operand.ty);
                    format!("suz_cmp_{index}")
                })
                .collect();
            let values: Vec<_> = operands
                .iter()
                .map(|operand| expression(operand, context))
                .collect();
            let compare = |index: usize| {
                if let Some(id) = structure {
                    return format!(
                        "{}suz_equal_{id}({}, {})",
                        if matches!(operators[index], crate::ast::ComparisonOperator::NotEqual) {
                            "!"
                        } else {
                            ""
                        },
                        names[index],
                        names[index + 1]
                    );
                }
                if text {
                    return if operators[index].is_equality() {
                        format!(
                            "{} {} {}",
                            names[index],
                            operators[index].symbol(),
                            names[index + 1]
                        )
                    } else {
                        format!(
                            "suz_text_compare({}, {}) {} 0",
                            names[index],
                            names[index + 1],
                            operators[index].symbol()
                        )
                    };
                }
                if floating {
                    return format!(
                        "{} {} {}",
                        names[index],
                        operators[index].symbol(),
                        names[index + 1]
                    );
                }
                format!(
                    "suz_compare_{}({}, {}) {} 0",
                    if unsigned { "unsigned" } else { "signed" },
                    names[index],
                    names[index + 1],
                    operators[index].symbol()
                )
            };
            let mut result = format!(
                "({} = {}, {} = {}, {}",
                names[0],
                values[0],
                names[1],
                values[1],
                compare(0)
            );
            for index in 1..operators.len() {
                write!(
                    result,
                    " && ({} = {}, {})",
                    names[index + 1],
                    values[index + 1],
                    compare(index)
                )
                .unwrap();
            }
            result.push(')');
            result
        }
        Value::Local(id) => context.local(id),
        Value::Integer { value, .. } if value == i64::MIN as i128 => {
            "(-INT64_C(9223372036854775807) - INT64_C(1))".into()
        }
        Value::Integer { value, .. } if value > i64::MAX as i128 => format!("UINT64_C({value})"),
        Value::Integer { value, .. } if value > i32::MAX as i128 => format!("INT64_C({value})"),
        Value::Integer { value, .. } if value < -(i32::MAX as i128) => {
            format!("(-INT64_C({}))", -value)
        }
        Value::Integer { value, .. } => value.to_string(),
    }
}
