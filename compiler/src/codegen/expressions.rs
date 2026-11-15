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
    if matches!(
        expr.value,
        Value::IntegerArithmetic {
            operation: IntegerOperation::Exact(_),
            ..
        }
    ) {
        return super::exact::stored(expr, context);
    }
    match expr.value {
        Value::GenericFloatLiteral(_) => {
            unreachable!("generic values are substituted before code generation")
        }
        Value::Exact(ref e) => format!(
            "suz_exact_require({}, {})",
            super::exact::tree(e, context),
            c_string(&context.source_name)
        ),
        Value::TextView(ref inner) => format!("suz_text_as_view({})", expression(inner, context)),
        Value::Intern {
            ref operand,
            location,
            ref constant,
        } => {
            if let Some(text) = constant {
                return super::text::constant(text, context);
            }
            context.uses_text = true;
            let id = context.temporaries.len();
            context.temporaries.push(operand.ty);
            let value = expression(operand, context);
            format!(
                "(suz_cmp_{id} = {value}, suz_text_intern(suz_cmp_{id}.bytes, suz_cmp_{id}.length, false, {}))",
                super::text::position(location, context)
            )
        }
        Value::Slice {
            ref target,
            ref start,
            ref end,
            ref handler,
            index,
        } => {
            if !index && start.is_none() && end.is_none() {
                return expression(target, context);
            }
            let (setup, condition, converted) = super::slices::parts(expr, context);
            let Some(CheckedHandler::Value(fallback)) = handler else {
                unreachable!("slice returns are lowered")
            };
            format!(
                "({setup}, {condition} ? {converted} : ({}))",
                expression(fallback, context)
            )
        }
        Value::Text { ref text, .. } => {
            if expr.ty == crate::ast::Type::Text {
                context.uses_text = true;
                format!(
                    "(suz_text_view){{{}, UINT64_C({}), UINT64_C({})}}",
                    c_string(text),
                    text.len(),
                    text.chars().count()
                )
            } else {
                super::text::constant(text, context)
            }
        }
        Value::Template {
            ref parts,
            location,
            ref constant,
        } => match constant {
            Some(text) => super::text::constant(text, context),
            None => super::text::template(parts, location, context),
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
            if condition == "true" {
                return format!("({setup}, {converted})");
            }
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
                        IntegerOperation::Exact(_) => unreachable!(),
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
                            "suz_cmp_{right_index} == 0 ? \"integer {name} by zero [rt.div-zero]\" : \"integer {name} overflow [rt.int-overflow]\""
                        )
                    } else if let IntegerOperation::Binary { operator, .. } = operation
                        && operator.is_shift()
                    {
                        let crate::ast::Type::Integer(ty) = expr.ty else {
                            unreachable!()
                        };
                        let IntegerOperation::Binary { right, .. } = operation else {
                            unreachable!()
                        };
                        let invalid = if right.ty == crate::ast::Type::Exact {
                            format!(
                                "suz_cmp_{right_index}.negative || suz_cmp_{right_index}.magnitude >= {}",
                                ty.bits()
                            )
                        } else {
                            format!("(uint64_t)suz_cmp_{right_index} >= {}", ty.bits())
                        };
                        format!(
                            "({invalid}) ? \"shift count out of range [rt.shift-count]\" : \"integer {name} overflow [rt.shift-overflow]\""
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
            if inner.ty == crate::ast::Type::Exact {
                let value = expression(inner, context);
                return super::exact::converted(expr.ty, &value, context);
            }
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
            let ty = target.ty;
            let target = expression(target, context);
            let mut result = expression(&arms.last().unwrap().1, context);
            for (pattern, value) in arms[..arms.len() - 1].iter().rev() {
                let pattern = expression(pattern.as_ref().unwrap(), context);
                let condition = super::expressions::equality(ty, &name, &pattern);
                result = format!("({condition} ? {} : {result})", expression(value, context));
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
                if inner.ty == crate::ast::Type::Exact {
                    format!("suz_exact_bits({})", expression(inner, context))
                } else {
                    expression(inner, context)
                }
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
            use crate::ast::{IntegerType, Type};
            let operands: Vec<_> = operands
                .iter()
                .map(|operand| {
                    if let Value::Exact(e) = &operand.value
                        && let crate::check::exact::ExactKind::Literal(n) = &e.kind
                        && let Some(value) = n.to_i128()
                    {
                        let ty = if i64::try_from(value).is_ok() {
                            Some(IntegerType::I64)
                        } else if u64::try_from(value).is_ok() {
                            Some(IntegerType::U64)
                        } else {
                            None
                        };
                        if let Some(ty) = ty {
                            return CheckedExpression {
                                ty: Type::Integer(ty),
                                value: Value::Integer { value, minus: None },
                            };
                        }
                    }
                    operand.clone()
                })
                .collect();
            let unsigned = operands[0]
                .ty
                .integer_storage()
                .is_some_and(|ty| ty.bounds().0 == 0);
            let text = operands[0].ty.is_text();
            let view = operands[0].ty == Type::Text;
            let floating = matches!(operands[0].ty, Type::Float(_));
            let structure = if let Type::Struct(id) = operands[0].ty {
                context.struct_equals.insert(id);
                Some(id)
            } else {
                None
            };
            if !floating
                && !text
                && structure.is_none()
                && !matches!(operands[0].ty, Type::Integer(_) | Type::Exact)
            {
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
                    if operators[index].is_equality() {
                        let equal = super::expressions::equality(
                            operands[index].ty,
                            &names[index],
                            &names[index + 1],
                        );
                        return if matches!(
                            operators[index],
                            crate::ast::ComparisonOperator::NotEqual
                        ) {
                            format!("!({equal})")
                        } else {
                            equal
                        };
                    }
                    let function = if view {
                        "suz_text_view_compare"
                    } else {
                        "suz_text_compare"
                    };
                    return format!(
                        "{function}({}, {}) {} 0",
                        names[index],
                        names[index + 1],
                        operators[index].symbol()
                    );
                }
                if matches!(
                    operands[index].ty,
                    Type::Exact | Type::Integer(_) | Type::Float(_)
                ) {
                    return super::exact::compare(
                        operands[index].ty,
                        &names[index],
                        operands[index + 1].ty,
                        &names[index + 1],
                        operators[index],
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

pub(super) fn equality(ty: crate::ast::Type, left: &str, right: &str) -> String {
    use crate::ast::Type;
    match ty {
        Type::Exact => format!("suz_exact_compare({left}, {right}) == 0"),
        Type::Text => format!("suz_text_view_equal({left}, {right})"),
        Type::Struct(id) => format!("suz_equal_{id}({left}, {right})"),
        _ => format!("{left} == {right}"),
    }
}
