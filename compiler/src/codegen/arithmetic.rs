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
    // A valid literal count cannot fail. Keep this common unsigned shift in C
    // instead of calling the checked runtime on every loop iteration.
    if matches!(operator, BinaryOperator::ShiftRight)
        && ty.bounds().0 == 0
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
    let op = match operator {
        BinaryOperator::Add => "add",
        BinaryOperator::Subtract => "sub",
        BinaryOperator::Multiply => "mul",
        BinaryOperator::Divide => "div",
        BinaryOperator::Remainder => "rem",
        BinaryOperator::ShiftLeft => "shl",
        BinaryOperator::ShiftRight => "shr",
        _ => unreachable!(),
    };
    let signed = ty.bounds().0 < 0;
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
    // Conversion to uint64_t preserves every valid count and makes negative
    // counts larger than any supported bit width, so the runtime rejects them.
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
