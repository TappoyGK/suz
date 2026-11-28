//! 検査済みの整数演算を、Cの演算または失敗検査付きのランタイム呼び出しへ変換する。
//!
//! 利用元: [`crate::codegen::expressions`]、[`crate::codegen::statements`]。

use super::{ExpressionContext, expressions::expression};
use crate::check::{CheckedExpression, IntegerOperation, Value};

pub(super) fn integer_parts(
    expr: &CheckedExpression,
    context: &mut ExpressionContext,
) -> (String, String, String) {
    use crate::ast::{BinaryOperator, IntegerType, Type};
    if matches!(
        expr.value,
        Value::IntegerArithmetic {
            operation: IntegerOperation::Exact(_),
            ..
        }
    ) {
        return super::exact::parts(expr, context);
    }
    let Value::IntegerArithmetic { operation, .. } = &expr.value else {
        unreachable!()
    };
    let Type::Integer(ty) = expr.ty else {
        unreachable!()
    };
    if let IntegerOperation::NegatedLiteral(value) = operation {
        let literal = CheckedExpression {
            ty: expr.ty,
            value: Value::Integer {
                value: *value,
                minus: None,
            },
        };
        return ("(void)0".into(), "1".into(), expression(&literal, context));
    }
    if let IntegerOperation::Negate(operand) = operation {
        let index = context.temporaries.len();
        context
            .temporaries
            .extend([operand.ty, Type::Integer(IntegerType::I64)]);
        let source = expression(operand, context);
        let limit = ty.name().to_uppercase().replace('I', "INT");
        return (
            format!("suz_cmp_{index} = {source}"),
            format!(
                "suz_try_sub_i64(0, suz_cmp_{index}, {limit}_MIN, {limit}_MAX, &suz_cmp_{})",
                index + 1
            ),
            format!("({})suz_cmp_{}", ty.c_name(), index + 1),
        );
    }
    let IntegerOperation::Binary {
        operator,
        left,
        right,
    } = operation
    else {
        unreachable!()
    };
    if matches!(operator, BinaryOperator::SaturatingShiftLeft) {
        context.saturating_helpers.insert(("shl", ty));
        let index = context.temporaries.len();
        context.temporaries.extend([left.ty, right.ty]);
        let exact_count = right.ty == Type::Exact;
        let unsigned_count = right
            .ty
            .integer_storage()
            .is_some_and(|r| r.bounds().0 == 0);
        let non_negative_const = matches!(right.value, Value::Integer { value, .. } if value >= 0);
        let left_code = expression(left, context);
        let right_code = expression(right, context);
        let condition = if unsigned_count || non_negative_const {
            "true".to_owned()
        } else if exact_count {
            format!("!suz_cmp_{}.negative", index + 1)
        } else {
            format!("suz_cmp_{} >= 0", index + 1)
        };
        let shift_arg = if exact_count {
            format!(
                "(suz_cmp_{}.magnitude >= {} ? {} : (uint64_t)suz_cmp_{}.magnitude)",
                index + 1,
                ty.bits(),
                ty.bits(),
                index + 1
            )
        } else {
            format!("(uint64_t)suz_cmp_{}", index + 1)
        };
        return (
            format!(
                "suz_cmp_{index} = {left_code}, suz_cmp_{} = {right_code}",
                index + 1
            ),
            condition,
            format!("suz_sat_shl_{}(suz_cmp_{index}, {shift_arg})", ty.name()),
        );
    }
    // 有効なリテラルのシフト量は失敗しない。この頻出する符号なしシフトはCで行い、
    // ループの各反復で検査付きランタイムを呼ぶのを避ける。
    if matches!(
        operator,
        BinaryOperator::ShiftRight | BinaryOperator::LogicalShiftRight
    ) && ty.bounds().0 == 0
        && let Value::Integer { value: count, .. } = right.value
        && (0..i128::from(ty.bits())).contains(&count)
    {
        let index = context.temporaries.len();
        context.temporaries.push(left.ty);
        let value = expression(left, context);
        return (
            format!("suz_cmp_{index} = {value}"),
            "true".into(),
            format!("({})(suz_cmp_{index} >> {count})", ty.c_name()),
        );
    }
    let signed = ty.bounds().0 < 0;
    let op = match operator {
        BinaryOperator::Add => "add",
        BinaryOperator::Subtract => "sub",
        BinaryOperator::Multiply => "mul",
        BinaryOperator::Divide => "div",
        BinaryOperator::Remainder => "rem",
        BinaryOperator::ShiftLeft => "shl",
        BinaryOperator::ShiftRight => "shr",
        BinaryOperator::LogicalShiftRight if signed => "lshr",
        BinaryOperator::LogicalShiftRight => "shr",
        _ => unreachable!(),
    };
    let wide = if signed {
        IntegerType::I64
    } else {
        IntegerType::U64
    };
    let index = context.temporaries.len();
    context
        .temporaries
        .extend([left.ty, right.ty, Type::Integer(wide)]);
    let left = expression(left, context);
    let exact_count = right.ty == Type::Exact;
    let right = expression(right, context);
    let limit = ty
        .name()
        .to_uppercase()
        .replace('I', "INT")
        .replace('U', "UINT");
    let bounds = if operator.is_shift() {
        ty.bits().to_string()
    } else if signed {
        format!("{limit}_MIN, {limit}_MAX")
    } else {
        format!("{limit}_MAX")
    };
    // uint64_tへの変換は有効なシフト量を保ち、負の値は対応する全ビット幅より大きくなるため、ランタイムで拒否される。
    let right_argument = if operator.is_shift() {
        if exact_count {
            format!(
                "(suz_cmp_{}.negative || suz_cmp_{}.magnitude >= {} ? UINT64_MAX : (uint64_t)suz_cmp_{}.magnitude)",
                index + 1,
                index + 1,
                ty.bits(),
                index + 1
            )
        } else {
            format!("(uint64_t)suz_cmp_{}", index + 1)
        }
    } else {
        format!("suz_cmp_{}", index + 1)
    };
    (
        format!("suz_cmp_{index} = {left}, suz_cmp_{} = {right}", index + 1),
        format!(
            "suz_try_{op}_{}(suz_cmp_{index}, {right_argument}, {bounds}, &suz_cmp_{})",
            wide.name(),
            index + 2
        ),
        format!("({})suz_cmp_{}", ty.c_name(), index + 2),
    )
}
