//! 検査済みの式をCへ変換し、短絡評価やオペランドの評価順を保つ。
//!
//! 利用元: [`crate::codegen::statements`]、[`crate::codegen::functions`]。

use super::{ExpressionContext, arithmetic::integer_parts, casts::cast_parts};
use crate::check::{CheckedExpression, CheckedHandler, IntegerOperation, Value};
use std::fmt::Write;

pub(super) fn c_string(text: &str) -> String {
    let mut result = String::from("\"");
    // ?もエスケープし、三文字表記によるCコンパイラの警告や文字の置換を防ぐ。
    for byte in text.bytes() {
        if (32..=126).contains(&byte) && !matches!(byte, b'"' | b'\\' | b'?') {
            result.push(char::from(byte));
        } else {
            write!(result, "\\{byte:03o}").unwrap();
        }
    }
    result.push('"');
    result
}

/// 両辺とも保存や呼び出し、失敗がなければ、Cの評価順でも読み取り結果は変わらない。
/// それ以外は従来どおり一時変数へ左から保存する。
fn unordered_read(expr: &CheckedExpression, context: &mut ExpressionContext) -> bool {
    match &expr.value {
        Value::Local(_) | Value::Float(_) | Value::Integer { .. } => true,
        Value::Binary { left, right, .. } if expr.ty.is_float() => {
            unordered_read(left, context) && unordered_read(right, context)
        }
        Value::Negate(value) => unordered_read(value, context),
        Value::ArrayRead {
            address,
            handler: None,
        } => {
            let Value::ArrayElement { target, indices } = &address.value else {
                return false;
            };
            matches!(target.value, Value::Local(id) if context.immutable_locals.contains(&id))
                && indices.iter().all(|(index, _, proven)| {
                    *proven && super::arrays::proven_index(index, context).is_some()
                })
        }
        _ => false,
    }
}

/// 有限のIEEE値は丸めを挟まない16進Cリテラルにする。NaNのペイロードは従来のビットコピーで保つ。
fn float_literal(bits: u64, ty: crate::ast::FloatType) -> Option<String> {
    use crate::ast::FloatType;
    let (fraction_bits, exponent_bits, bias, suffix) = match ty {
        FloatType::F32 => (23, 8, 127, "f"),
        FloatType::F64 => (52, 11, 1023, ""),
    };
    let exponent = (bits >> fraction_bits) & ((1 << exponent_bits) - 1);
    if exponent == (1 << exponent_bits) - 1 {
        return None;
    }
    let sign = if bits >> (fraction_bits + exponent_bits) != 0 {
        "-"
    } else {
        ""
    };
    let fraction = bits & ((1 << fraction_bits) - 1);
    let significand = fraction | if exponent == 0 { 0 } else { 1 << fraction_bits };
    let power = exponent.max(1) as i32 - bias - fraction_bits;
    Some(format!("({sign}0x{significand:x}p{power}{suffix})"))
}

pub(super) fn expression(expr: &CheckedExpression, context: &mut ExpressionContext) -> String {
    context.uses_c_pointer |= matches!(expr.ty, crate::ast::Type::CPointer);
    context.uses_c_integer |= matches!(expr.ty, crate::ast::Type::CInteger(_));
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
        Value::Array { .. } => super::arrays::constant(expr, context),
        Value::ArrayElement { .. } => super::arrays::address(expr, context),
        Value::ArrayView { ref handler, .. } | Value::ArrayRead { ref handler, .. } => {
            let (setup, condition, value) = super::arrays::read_parts(expr, context);
            if handler.is_none() {
                return format!("({setup}, (void)({condition}), {value})");
            }
            let Some(CheckedHandler::Value(fallback)) = handler else {
                unreachable!("array return handlers are lowered")
            };
            format!(
                "({setup}, {condition} ? {value} : ({}))",
                expression(fallback, context)
            )
        }
        Value::Receiver { ref target, .. } => expression(target, context),
        Value::TextAppend {
            ref left,
            ref right,
            location,
        } => super::text::append(left, right, location, context),
        Value::TextBuffer {
            ref capacity,
            ref value,
            location,
        } => super::text::buffer(capacity.as_deref(), value, location, context),
        Value::New {
            ref value,
            location,
        } => super::arena::allocate(value, expr.ty, location, context),
        Value::Deref(ref value) => format!("(*({}))", expression(value, context)),
        Value::NonNil(ref value) if value.ty.array_id().is_some() => {
            format!("(({}).data != NULL)", expression(value, context))
        }
        Value::Nil if expr.ty.array_id().is_some() => format!("({}){{0}}", expr.ty.c_name()),
        Value::NonNil(ref value) => format!("({} != NULL)", expression(value, context)),
        Value::Nil => "NULL".into(),
        Value::CBytes(ref operand) => super::ffi::bytes(operand, context),
        Value::CByteLen {
            ref operand,
            location,
        } => super::ffi::byte_len(operand, location, context),
        Value::CIntegerLiteral { value, location } => {
            super::ffi::literal(value, expr.ty, location, context)
        }
        Value::Panic {
            ref parts,
            location,
        } => super::logs::panic_expression(parts, location, expr.ty, context),
        Value::HandledCall {
            ref call,
            ref handler,
        } => super::errors::expression(call, handler, context),
        Value::Closure(ref closure) => super::closures::emit(closure, expr.ty, context),
        Value::Function { id, .. } => format!("suz_fn_{id}"),
        Value::FunctionBorrow(ref value) => super::functions::borrow(value, context),
        Value::Invoke {
            ref target,
            ref arguments,
            ..
        } => super::functions::invoke(target, arguments, context, None),
        Value::GenericNumericLiteral { .. } => {
            unreachable!("generic values are substituted before code generation")
        }
        Value::Exact(ref e) => {
            let tree = super::exact::tree(e, context);
            if super::exact::can_fail(e) {
                format!(
                    "suz_exact_require({tree}, {})",
                    c_string(&context.source_name)
                )
            } else {
                tree
            }
        }
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
                    "(suz_text_view){{{}, UINT64_C({}), UINT64_C({}), NULL}}",
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
            Some(text) if expr.ty == crate::ast::Type::Text => super::text::static_view(text),
            Some(text) => super::text::constant(text, context),
            None => super::text::template(parts, location, context),
        },
        Value::With {
            ref target,
            ref fields,
        } => super::structs::with(target, fields, context),
        Value::Struct { ref fields } => {
            let crate::ast::Type::Struct(id) = expr.ty else {
                unreachable!()
            };
            super::structs::literal(id, fields, context)
        }
        Value::Field { ref target, index } => {
            let name = super::structs::field_name(target.ty, index, context);
            format!("({}).{name}", expression(target, context))
        }
        Value::Void => String::new(),
        Value::Call {
            id, ref arguments, ..
        } => super::functions::call(id, arguments, context, None),
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
                Some(CheckedHandler::Return { .. }) => {
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
                            BinaryOperator::LogicalShiftRight => "logical right shift",
                            BinaryOperator::SaturatingShiftLeft => "saturating left shift",
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
                        if matches!(operator, BinaryOperator::ShiftLeft) {
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
                            "\"shift count out of range [rt.shift-count]\"".into()
                        }
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
            kind,
            ref operand,
            ref fallback,
            ..
        } => {
            if let Some(value) = constant_struct {
                return expression(value, context);
            }
            if kind == crate::ast::CastKind::Saturating
                && let (crate::ast::Type::Float(from), crate::ast::Type::Integer(to)) =
                    (operand.ty, expr.ty)
                && unordered_read(fallback, context)
            {
                context.float_types.insert(from);
                context.float_sat_casts.insert((from, to));
                if unordered_read(operand, context) {
                    return format!(
                        "suz_sat_f2i_{}_{}({}, ({})({}))",
                        from.name(),
                        to.name(),
                        expression(operand, context),
                        to.c_name(),
                        expression(fallback, context)
                    );
                }
                let index = context.temporaries.len();
                context.temporaries.push(operand.ty);
                let op_code = expression(operand, context);
                let fb_code = expression(fallback, context);
                return format!(
                    "(suz_cmp_{index} = {op_code}, suz_sat_f2i_{}_{}(suz_cmp_{index}, ({})({fb_code})))",
                    from.name(),
                    to.name(),
                    to.c_name(),
                );
            }
            let (setup, condition, converted) =
                cast_parts(operand, expr.ty, kind, location, context);
            let fallback = expression(fallback, context);
            format!("({setup}, {condition} ? {converted} : ({fallback}))")
        }
        Value::Cast(ref inner) => {
            if expr.ty.is_readonly_array() && !inner.ty.is_readonly_array() {
                return super::arrays::readonly(inner, context);
            }
            if expr.ty.array_id().is_some() {
                return expression(inner, context);
            }
            if let Some((_, value)) = super::exact::native_value(inner, context) {
                if let crate::ast::Type::Float(ty) = expr.ty {
                    context.float_types.insert(ty);
                }
                return format!("({})({value})", expr.ty.c_name());
            }
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

        Value::ArrayLength {
            ref target,
            dimension,
        } => {
            format!("({}).lengths[{dimension}]", expression(target, context))
        }
        Value::ArrayDimensionGuard {
            ref name,
            ref left,
            ref right,
            location,
        } => {
            context.uses_arena = true;
            let left = expression(left, context);
            let right = expression(right, context);
            format!(
                "(({left}) == ({right}) ? (void)0 : suz_array_dimensions({}, {}, {}, {}, {left}, {right}))",
                c_string(&context.source_name),
                location.line,
                location.column,
                c_string(name)
            )
        }
        Value::Len(ref inner) => {
            let value = expression(inner, context);
            // 実体またはビューが保持する個数を読む。オペランドは一度だけ評価する。
            if inner.ty.array_id().is_some() {
                format!("({value}).lengths[0]")
            } else if inner.ty == crate::ast::Type::Symbol {
                format!("suz_text_count({value})")
            } else {
                format!("((int32_t)({value}).scalars)")
            }
        }
        Value::Negate(ref inner) => format!("(-({}))", expression(inner, context)),
        Value::Float(bits) => {
            let crate::ast::Type::Float(ty) = expr.ty else {
                unreachable!()
            };
            context.float_types.insert(ty);
            if let Some(literal) = float_literal(bits, ty) {
                return literal;
            }
            context.float_helpers.insert(ty);
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
            if matches!(expr.ty, crate::ast::Type::CInteger(_)) {
                return super::ffi::truncate(inner, expr.ty, context);
            }
            let crate::ast::Type::Integer(ty) = expr.ty else {
                unreachable!()
            };
            context.bit_helpers.insert(ty);
            format!(
                "suz_bits_{}((uint64_t)({}))",
                ty.name(),
                if let Some((_, value)) = super::exact::native_value(inner, context) {
                    value
                } else if inner.ty == crate::ast::Type::Exact {
                    format!("suz_exact_bits({})", expression(inner, context))
                } else {
                    expression(inner, context)
                }
            )
        }
        Value::SaturatingCast(ref inner) => {
            use crate::ast::Type;
            if matches!(expr.ty, Type::CInteger(_)) || matches!(inner.ty, Type::CInteger(_)) {
                return super::ffi::saturating_cast(inner, expr.ty, context);
            }
            let Type::Integer(to) = expr.ty else {
                unreachable!()
            };
            let (to_min, to_max) = to.bounds();
            let c_to = to.c_name();
            let limit = to
                .name()
                .to_uppercase()
                .replace('I', "INT")
                .replace('U', "UINT");
            let native_ty = super::exact::native_type(inner);
            if inner.ty == Type::Exact && native_ty.is_none() {
                let id = context.temporaries.len();
                context.temporaries.push(Type::Exact);
                let val = expression(inner, context);
                let neg_branch = if to_min == 0 {
                    format!("({c_to})0")
                } else {
                    let abs_min = to_min.unsigned_abs();
                    format!(
                        "(suz_cmp_{id}.magnitude >= {abs_min}ULL ? {limit}_MIN : ({c_to})-(int64_t)suz_cmp_{id}.magnitude)"
                    )
                };
                return format!(
                    "(suz_cmp_{id} = {val}, suz_cmp_{id}.negative ? {neg_branch} : (suz_cmp_{id}.magnitude >= {to_max}ULL ? {limit}_MAX : ({c_to})suz_cmp_{id}.magnitude))"
                );
            }
            let inner_ty = native_ty.unwrap_or(inner.ty);
            if let Type::Float(from) = inner_ty {
                context.float_types.insert(from);
                context.float_sat_casts.insert((from, to));
                return format!(
                    "suz_sat_f2i_{}_{}({}, ({c_to})0)",
                    from.name(),
                    to.name(),
                    expression(inner, context)
                );
            }
            let Type::Integer(from) = inner_ty else {
                unreachable!()
            };
            let (from_min, from_max) = from.bounds();
            if from_min >= to_min && from_max <= to_max {
                let val = if let Some(ty) = native_ty {
                    super::exact::emit_native(inner, ty, context)
                } else {
                    expression(inner, context)
                };
                return format!("({c_to})({val})");
            }
            let id = context.temporaries.len();
            context.temporaries.push(inner_ty);
            let val = if let Some(ty) = native_ty {
                super::exact::emit_native(inner, ty, context)
            } else {
                expression(inner, context)
            };
            let local = format!("suz_cmp_{id}");
            let mut body = format!("({c_to}){local}");
            if from_max > to_max {
                let prefix = if from.c_signed() {
                    "INT64_C"
                } else {
                    "UINT64_C"
                };
                body = format!("({local} > {prefix}({to_max}) ? {limit}_MAX : {body})");
            }
            if from_min < to_min {
                let min_expr = if to_min == 0 {
                    format!("({c_to})0")
                } else {
                    format!("{limit}_MIN")
                };
                body = format!("({local} < INT64_C({to_min}) ? {min_expr} : {body})");
            }
            format!("({local} = {val}, {body})")
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
        Value::WrapNegate(ref inner) => {
            let crate::ast::Type::Integer(ty) = expr.ty else {
                unreachable!()
            };
            context.bit_helpers.insert(ty);
            format!(
                "suz_bits_{}(UINT64_C(0) - (uint64_t)({}))",
                ty.name(),
                expression(inner, context)
            )
        }
        Value::SaturatingNegate(ref inner) => {
            let crate::ast::Type::Integer(ty) = expr.ty else {
                unreachable!()
            };
            context.saturating_helpers.insert(("neg", ty));
            format!("suz_sat_neg_{}({})", ty.name(), expression(inner, context))
        }
        Value::Binary {
            operator,
            ref left,
            ref right,
        } => {
            if expr.ty.is_float() && unordered_read(left, context) && unordered_read(right, context)
            {
                let crate::ast::Type::Float(ty) = expr.ty else {
                    unreachable!()
                };
                return match operator {
                    crate::ast::BinaryOperator::Remainder => {
                        context.uses_float_rem = true;
                        let fmod_fn = match ty {
                            crate::ast::FloatType::F32 => "fmodf",
                            crate::ast::FloatType::F64 => "fmod",
                        };
                        format!(
                            "{fmod_fn}(({}), ({}))",
                            expression(left, context),
                            expression(right, context)
                        )
                    }
                    crate::ast::BinaryOperator::EuclideanRemainder => {
                        context.float_euclid.insert(ty);
                        format!(
                            "suz_euclid_{}(({}), ({}))",
                            ty.name(),
                            expression(left, context),
                            expression(right, context)
                        )
                    }
                    _ => format!(
                        "({})(({}) {} ({}))",
                        expr.ty.c_name(),
                        expression(left, context),
                        operator.c_symbol(),
                        expression(right, context)
                    ),
                };
            }
            let right_native = super::exact::native_type(right);
            let right_ty = right_native.unwrap_or(right.ty);
            let index = context.temporaries.len();
            context.temporaries.extend([left.ty, right_ty]);
            let right_is_exact = right_ty == crate::ast::Type::Exact;
            let left = expression(left, context);
            let right = if let Some(ty) = right_native {
                super::exact::emit_native(right, ty, context)
            } else {
                expression(right, context)
            };
            let result = match expr.ty {
                crate::ast::Type::Integer(ty) => {
                    use crate::ast::BinaryOperator;
                    let bits = ty.bits();
                    let shift_mask = bits - 1;
                    let shift_count = if right_is_exact {
                        format!("(suz_exact_bits(suz_cmp_{}) & {shift_mask})", index + 1)
                    } else {
                        format!("((uint64_t)suz_cmp_{} & {shift_mask})", index + 1)
                    };
                    match operator {
                        BinaryOperator::WrapShiftLeft => {
                            context.bit_helpers.insert(ty);
                            format!(
                                "suz_bits_{}((uint64_t)suz_cmp_{index} << {shift_count})",
                                ty.name()
                            )
                        }
                        BinaryOperator::WrapShiftRight => {
                            if ty.bounds().0 < 0 {
                                format!(
                                    "(suz_cmp_{index} < 0 ? ({})(-INT64_C(1) - (int64_t)((uint64_t)(-(int64_t)(suz_cmp_{index} + 1)) >> {shift_count})) : ({})((uint64_t)(int64_t)suz_cmp_{index} >> {shift_count}))",
                                    ty.c_name(),
                                    ty.c_name(),
                                )
                            } else {
                                context.bit_helpers.insert(ty);
                                format!(
                                    "suz_bits_{}((uint64_t)suz_cmp_{index} >> {shift_count})",
                                    ty.name()
                                )
                            }
                        }
                        BinaryOperator::WrapLogicalShiftRight => {
                            context.bit_helpers.insert(ty);
                            let value_mask = u64::MAX >> (64 - bits);
                            format!(
                                "suz_bits_{}(((uint64_t)suz_cmp_{index} & UINT64_C({value_mask})) >> {shift_count})",
                                ty.name()
                            )
                        }
                        BinaryOperator::SaturatingAdd => {
                            context.saturating_helpers.insert(("add", ty));
                            format!(
                                "suz_sat_add_{}(suz_cmp_{index}, suz_cmp_{})",
                                ty.name(),
                                index + 1
                            )
                        }
                        BinaryOperator::SaturatingSubtract => {
                            context.saturating_helpers.insert(("sub", ty));
                            format!(
                                "suz_sat_sub_{}(suz_cmp_{index}, suz_cmp_{})",
                                ty.name(),
                                index + 1
                            )
                        }
                        BinaryOperator::SaturatingMultiply => {
                            context.saturating_helpers.insert(("mul", ty));
                            format!(
                                "suz_sat_mul_{}(suz_cmp_{index}, suz_cmp_{})",
                                ty.name(),
                                index + 1
                            )
                        }
                        BinaryOperator::SaturatingShiftLeft => {
                            context.saturating_helpers.insert(("shl", ty));
                            format!(
                                "suz_sat_shl_{}(suz_cmp_{index}, (uint64_t)suz_cmp_{})",
                                ty.name(),
                                index + 1
                            )
                        }
                        _ => {
                            context.bit_helpers.insert(ty);
                            format!(
                                "suz_bits_{}((uint64_t)suz_cmp_{index} {} (uint64_t)suz_cmp_{})",
                                ty.name(),
                                operator.c_symbol(),
                                index + 1
                            )
                        }
                    }
                }
                crate::ast::Type::Float(ty) => match operator {
                    crate::ast::BinaryOperator::Remainder => {
                        context.uses_float_rem = true;
                        let fmod_fn = match ty {
                            crate::ast::FloatType::F32 => "fmodf",
                            crate::ast::FloatType::F64 => "fmod",
                        };
                        format!("{fmod_fn}(suz_cmp_{index}, suz_cmp_{})", index + 1)
                    }
                    crate::ast::BinaryOperator::EuclideanRemainder => {
                        context.float_euclid.insert(ty);
                        format!(
                            "suz_euclid_{}(suz_cmp_{index}, suz_cmp_{})",
                            ty.name(),
                            index + 1
                        )
                    }
                    _ => format!(
                        "({})(suz_cmp_{index} {} suz_cmp_{})",
                        ty.c_name(),
                        operator.c_symbol(),
                        index + 1
                    ),
                },
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
            ref methods,
            ref operators,
            ref operands,
            ..
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
            // 宣言には評価の副作用がない。代入は式の中に残し、論理演算の短絡評価とループの再評価を保つ。
            // 演算子メソッドへポインタ渡しする不変の構造体は一時変数へコピーせず、直接アドレスを渡す。
            let pointer_struct: Vec<bool> = operands
                .iter()
                .map(|operand| super::functions::pass_struct_by_pointer(operand.ty, context))
                .collect();
            let direct_struct: Vec<bool> = operands
                .iter()
                .enumerate()
                .map(|(index, operand)| {
                    if !pointer_struct[index]
                        || !super::functions::immutable_struct_place(operand, context)
                    {
                        return false;
                    }
                    let left_method =
                        index == 0 || methods.get(index - 1).is_some_and(Option::is_some);
                    let right_method =
                        index >= operators.len() || methods.get(index).is_some_and(Option::is_some);
                    left_method && right_method
                })
                .collect();
            let native_types: Vec<_> = operands.iter().map(super::exact::native_type).collect();
            let operand_types: Vec<_> = operands
                .iter()
                .enumerate()
                .map(|(i, operand)| native_types[i].unwrap_or(operand.ty))
                .collect();
            let names: Vec<_> = operands
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    if direct_struct[i] {
                        String::new()
                    } else {
                        let index = context.temporaries.len();
                        context.temporaries.push(operand_types[i]);
                        format!("suz_cmp_{index}")
                    }
                })
                .collect();
            let values: Vec<_> = operands
                .iter()
                .enumerate()
                .map(|(i, operand)| {
                    if let Some(ty) = native_types[i] {
                        super::exact::emit_native(operand, ty, context)
                    } else {
                        expression(operand, context)
                    }
                })
                .collect();
            let method_argument = |i: usize| -> String {
                if direct_struct[i] {
                    format!("&({})", values[i])
                } else if pointer_struct[i] {
                    format!("&{}", names[i])
                } else {
                    names[i].clone()
                }
            };
            let mut compare = |index: usize| {
                if let Some(Some(id)) = methods.get(index) {
                    return format!(
                        "suz_fn_{id}({}, {})",
                        method_argument(index),
                        method_argument(index + 1)
                    );
                }
                let ty = operand_types[index];
                let unsigned = ty.integer_storage().is_some_and(|ty| ty.bounds().0 == 0);
                let text = ty.is_text();
                let view = ty == Type::Text;
                let floating = matches!(ty, Type::Float(_));
                let structure = if let Type::Struct(id) = ty {
                    context.struct_equals.insert(id);
                    Some(id)
                } else {
                    None
                };
                if !floating
                    && !text
                    && structure.is_none()
                    && !matches!(
                        ty,
                        Type::Integer(_) | Type::Range(_) | Type::Exact | Type::CInteger(_)
                    )
                {
                    context.helpers.insert(unsigned);
                }

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
                        let equal =
                            super::expressions::equality(ty, &names[index], &names[index + 1]);
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
                    ty,
                    Type::Exact | Type::Integer(_) | Type::Range(_) | Type::Float(_)
                ) {
                    return super::exact::compare(
                        ty,
                        &names[index],
                        operand_types[index + 1],
                        &names[index + 1],
                        operators[index],
                    );
                }
                if matches!(ty, Type::CInteger(_)) {
                    // 同じC型同士なので、その型の符号と幅のまま比較する。
                    // int64_tの比較ヘルパーへ渡すとsize_tなどの上半分を負数にしてしまう。
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
            let mut first = Vec::new();
            if !direct_struct[0] {
                first.push(format!("{} = {}", names[0], values[0]));
            }
            if !direct_struct[1] {
                first.push(format!("{} = {}", names[1], values[1]));
            }
            first.push(compare(0));
            let mut result = format!("({}", first.join(", "));
            for index in 1..operators.len() {
                if direct_struct[index + 1] {
                    write!(result, " && ({})", compare(index)).unwrap();
                } else {
                    write!(
                        result,
                        " && ({} = {}, {})",
                        names[index + 1],
                        values[index + 1],
                        compare(index)
                    )
                    .unwrap();
                }
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
