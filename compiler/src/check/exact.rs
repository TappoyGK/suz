use super::{
    ir::{CheckedExpression, IntegerOperation, Value},
    scope::Scope,
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{BinaryOperator, Expression, ExpressionKind, FloatType, IntegerType, Location, Type},
    numeric::{Int, Range},
};

#[derive(Clone)]
pub(crate) struct Exact {
    pub arithmetic: bool,
    pub kind: ExactKind,
    pub range: Option<Range>,
    pub location: Location,
}
#[derive(Clone)]
pub(crate) enum ExactKind {
    Literal(Int),
    Leaf(Box<CheckedExpression>),
    Negate(Box<Exact>),
    Binary(BinaryOperator, Box<Exact>, Box<Exact>),
}
impl Exact {
    pub fn leaves(&self, f: &mut impl FnMut(&CheckedExpression)) {
        match &self.kind {
            ExactKind::Leaf(e) => f(e),
            ExactKind::Negate(e) => e.leaves(f),
            ExactKind::Binary(_, a, b) => {
                a.leaves(f);
                b.leaves(f);
            }
            _ => {}
        }
    }
}
pub(super) fn is_operator(expr: &Expression) -> bool {
    match &expr.kind {
        ExpressionKind::Group(e) => is_operator(e),
        ExpressionKind::Negate(_) => true,
        ExpressionKind::Integer { .. } => false,
        ExpressionKind::Binary { operator, .. } => matches!(
            operator,
            BinaryOperator::Add
                | BinaryOperator::Subtract
                | BinaryOperator::Multiply
                | BinaryOperator::Divide
                | BinaryOperator::Remainder
        ),
        _ => false,
    }
}
pub(super) fn integer(expr: &Expression, bindings: &Scope) -> bool {
    !super::expression::has_float_literal(expr, bindings)
        && matches!(
            super::expression::operand_type(expr, bindings),
            None | Some(Type::Integer(_))
        )
}
fn error(location: Location) -> Diagnostic {
    location.error(ErrorKind::InvalidSource, "exact integer expression exceeds 128 bits; store an intermediate value or use as [arith.width]")
}
fn build(expr: &Expression, bindings: &Scope) -> Result<Exact, Diagnostic> {
    let location = expr.location;
    let (kind, range) = match &expr.kind {
        ExpressionKind::Group(e) => return build(e, bindings),
        ExpressionKind::Integer { digits, negative } => {
            let value = Int::new(*negative, digits.parse().map_err(|_| error(location))?);
            (ExactKind::Literal(value), Some(Range::point(value)))
        }
        ExpressionKind::Negate(e) => {
            let e = build(e, bindings)?;
            let range = e.range.map(|r| Range {
                min: r.max.neg(),
                max: r.min.neg(),
            });
            (ExactKind::Negate(Box::new(e)), range)
        }
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } if is_operator(expr) => {
            let left = build(left, bindings)?;
            let right = build(right, bindings)?;
            let range = match (left.range, right.range) {
                (Some(a), Some(b)) => a.binary(*operator, b).map_err(|_| error(location))?,
                _ => None,
            };
            (
                ExactKind::Binary(*operator, Box::new(left), Box::new(right)),
                range,
            )
        }
        _ => {
            let value = super::expression::expression(expr, None, bindings)?;
            let Type::Integer(ty) = value.ty else {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    if matches!(value.ty, Type::Float(_)) {
                        "integer arithmetic requires an integer operand [arith.exact]"
                    } else {
                        "integer arithmetic requires integer operands [expr.operator-types]"
                    },
                ));
            };
            let (min, max) = ty.bounds();
            (
                ExactKind::Leaf(Box::new(value)),
                Some(Range::bounds(min, max)),
            )
        }
    };
    if range.is_some_and(|r| !r.fits()) {
        return Err(error(location));
    }
    Ok(Exact {
        arithmetic: is_operator(expr)
            || matches!(expr.kind, ExpressionKind::Integer { negative: true, .. }),
        kind,
        range,
        location,
    })
}
pub(super) fn check(
    expr: &Expression,
    expected: Option<Type>,
    bindings: &Scope,
    handled: bool,
) -> Result<CheckedExpression, Diagnostic> {
    let exact = build(expr, bindings)?;
    if exact.range.is_none() && !handled {
        return Err(exact.location.error(
            ErrorKind::InvalidSource,
            "integer expression always divides by zero and requires or [arith.div-zero-always]",
        ));
    }
    let ty = if let Some(ty) = expected {
        ty
    } else {
        let mut known = None;
        let mut incompatible = false;
        exact.leaves(&mut |v| {
            known = match known {
                None => Some(v.ty),
                Some(a) => match join(a, v.ty) {
                    Some(t) => Some(t),
                    None => {
                        incompatible = true;
                        Some(a)
                    }
                },
            };
        });
        if incompatible {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                "integer operands have no common storage type [conv.join]",
            ));
        }
        known.unwrap_or(Type::Integer(IntegerType::I64))
    };
    if ty == Type::Exact {
        return Ok(CheckedExpression {
            ty,
            value: Value::Exact(Box::new(exact)),
        });
    }
    if let Type::Float(float) = ty {
        if exact
            .range
            .is_some_and(|r| !r.exact_float(float == FloatType::F32))
        {
            return Err(exact.location.error(
                ErrorKind::InvalidSource,
                "integer range is not exactly representable; use as [arith.store-float]",
            ));
        }
    } else if !matches!(ty, Type::Integer(_)) {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            "integer arithmetic requires a numeric destination [type.expected]",
        ));
    }
    let location = exact.location;
    Ok(CheckedExpression {
        ty,
        value: Value::IntegerArithmetic {
            operation: IntegerOperation::Exact(Box::new(exact)),
            location,
            handler: None,
        },
    })
}
pub(super) fn use_value(
    expr: &Expression,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    if is_operator(expr) && integer(expr, bindings) {
        check(expr, Some(Type::Exact), bindings, false)
    } else if let ExpressionKind::Group(e) = &expr.kind {
        use_value(e, bindings)
    } else {
        super::expression::expression(expr, None, bindings)
    }
}
pub(super) fn value_range(expr: &CheckedExpression) -> Option<Range> {
    match &expr.value {
        Value::Exact(e) => e.range,
        _ => expr.ty.integer_bounds().map(|(a, b)| Range::bounds(a, b)),
    }
}
pub(super) fn evaluate(e: &Exact) -> Option<Result<Int, Location>> {
    Some(match &e.kind {
        ExactKind::Literal(v) => Ok(*v),
        ExactKind::Leaf(v) => Ok(Int::from_i128(super::constants::constant_integer(v).ok()??)),
        ExactKind::Negate(v) => evaluate(v)?.map(Int::neg),
        ExactKind::Binary(op, a, b) => {
            let a = match evaluate(a)? {
                Ok(v) => v,
                Err(l) => return Some(Err(l)),
            };
            let b = match evaluate(b)? {
                Ok(v) => v,
                Err(l) => return Some(Err(l)),
            };
            use BinaryOperator::*;
            match op {
                Add => a.add(b),
                Subtract => a.sub(b),
                Multiply => a.mul(b),
                Divide => a.div(b),
                Remainder => a.rem(b),
                _ => unreachable!(),
            }
            .ok_or(e.location)
        }
    })
}
pub(super) fn constant(expr: &CheckedExpression) -> Option<Int> {
    if let Value::Exact(e) = &expr.value {
        evaluate(e)?.ok()
    } else {
        super::constants::constant_integer(expr)
            .ok()?
            .map(Int::from_i128)
    }
}
pub(super) fn widening(from: Type, to: Type) -> bool {
    match (from, to) {
        (a, b) if a == b => true,
        (Type::Integer(a), Type::Integer(b)) => {
            let (min, max) = a.bounds();
            let (lo, hi) = b.bounds();
            min >= lo && max <= hi
        }
        (Type::Integer(a), Type::Float(b)) => {
            let (lo, hi) = a.bounds();
            Range::bounds(lo, hi).exact_float(b == FloatType::F32)
        }
        (Type::Float(FloatType::F32), Type::Float(FloatType::F64)) => true,
        _ => false,
    }
}
pub(super) fn join(a: Type, b: Type) -> Option<Type> {
    if a == b {
        return Some(a);
    }
    if a.is_text() && b.is_text() {
        return Some(Type::Text);
    }
    if matches!((a, b), (Type::Integer(_), Type::Integer(_))) {
        for ty in [
            IntegerType::U8,
            IntegerType::I8,
            IntegerType::U16,
            IntegerType::I16,
            IntegerType::U32,
            IntegerType::I32,
            IntegerType::U64,
            IntegerType::I64,
        ] {
            let ty = Type::Integer(ty);
            if widening(a, ty) && widening(b, ty) {
                return Some(ty);
            }
        }
    } else if matches!(a, Type::Float(_) | Type::Integer(_))
        && matches!(b, Type::Float(_) | Type::Integer(_))
    {
        for ty in [Type::Float(FloatType::F32), Type::Float(FloatType::F64)] {
            if widening(a, ty) && widening(b, ty) {
                return Some(ty);
            }
        }
    }
    None
}
pub(super) fn cast(
    operand: CheckedExpression,
    target: Type,
    wrap: bool,
    fallback: &Option<crate::ast::FailureHandler>,
    location: Location,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    use crate::ast::FailureHandler;
    if wrap {
        if fallback.is_some() {
            return Err(location.error(
                ErrorKind::InvalidSource,
                "integer as% does not accept or [expr.cast-wrap-handler-syntax]",
            ));
        }
        if !matches!(target, Type::Integer(_)) {
            return Err(location.error(
                ErrorKind::InvalidSource,
                "as% requires an integer target [expr.as-trunc]",
            ));
        }
        return Ok(CheckedExpression {
            ty: target,
            value: Value::Truncate(Box::new(operand)),
        });
    }
    if !matches!(
        target,
        Type::Integer(_) | Type::Float(_) | Type::Enum(_) | Type::Char
    ) {
        return Err(location.error(
            ErrorKind::InvalidSource,
            "invalid exact integer conversion [expr.as]",
        ));
    }
    let fits = |r: Range| match target {
        Type::Float(_) => true,
        _ => r.fits_type(target),
    };
    let safe = value_range(&operand).is_some_and(fits);
    if let Some(handler) = fallback {
        let value = match handler {
            FailureHandler::Value(v) => Value::CheckedCast {
                location,
                constant_struct: None,
                wrap: false,
                operand: Box::new(operand),
                fallback: Box::new(super::expression::coerced(v, Some(target), bindings)?),
            },
            FailureHandler::Return { value, .. } => Value::CastReturn {
                location,
                wrap: false,
                operand: Box::new(operand),
                return_value: Box::new(super::functions::return_value(value, bindings)?),
            },
        };
        return Ok(CheckedExpression { ty: target, value });
    }
    if !safe && !constant(&operand).is_some_and(|n| fits(Range::point(n))) {
        return Err(super::generic::missing_handler(
            location,
            bindings,
            super::constants::is_constant(&operand),
        ));
    }
    Ok(CheckedExpression {
        ty: target,
        value: Value::Cast(Box::new(operand)),
    })
}
pub(super) fn constant_comparison(
    a: &CheckedExpression,
    b: &CheckedExpression,
    op: crate::ast::ComparisonOperator,
) -> Option<bool> {
    if super::constants::is_constant(a) && super::constants::is_constant(b) {
        return None;
    }
    let a = value_range(a)?;
    let b = value_range(b)?;
    use crate::ast::ComparisonOperator::*;
    match op {
        Less if a.max < b.min => Some(true),
        Less if a.min >= b.max => Some(false),
        LessEqual if a.max <= b.min => Some(true),
        LessEqual if a.min > b.max => Some(false),
        Greater if a.min > b.max => Some(true),
        Greater if a.max <= b.min => Some(false),
        GreaterEqual if a.min >= b.max => Some(true),
        GreaterEqual if a.max < b.min => Some(false),
        Equal | NotEqual if a.max < b.min || b.max < a.min => Some(matches!(op, NotEqual)),
        Equal | NotEqual if a.min == a.max && a.min == b.min && b.min == b.max => {
            Some(matches!(op, Equal))
        }
        _ => None,
    }
}

pub(super) fn comparison_value(
    expr: &Expression,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    match &expr.kind {
        ExpressionKind::Integer { .. } => check(expr, Some(Type::Exact), bindings, false),
        ExpressionKind::Group(e) => comparison_value(e, bindings),
        _ => use_value(expr, bindings),
    }
}
