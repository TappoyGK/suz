use super::{
    ir::{CheckedExpression, IntegerOperation, Value},
    scope::Scope,
};
use crate::check::generic::domain;
use crate::{
    Diagnostic, ErrorKind,
    ast::{BinaryOperator, Expression, ExpressionKind, FloatType, IntegerType, Location, Type},
    numeric::{Int, Range},
};

// Per substitution: Err is an unrepresentable range; Ok(None) always divides by zero.
type SubstitutionRanges = [Result<Option<Range>, ()>; domain::NUMERIC_COMBINATIONS];

#[derive(Clone)]
pub(crate) struct Exact {
    pub arithmetic: bool,
    pub kind: ExactKind,
    pub range: Option<Range>,
    pub domains: Option<Box<SubstitutionRanges>>,
    pub location: Location,
}
#[derive(Clone)]
pub(crate) enum ExactKind {
    Literal(Int),
    Leaf(Box<CheckedExpression>),
    Negate(Box<Exact>),
    Binary(BinaryOperator, Box<Exact>, Box<Exact>),
}
// The same definition visits read-only/mutable nodes and leaves in one pass.
// rustfmt does not format this macro body; keep its layout consistent manually.
macro_rules! exact_visitor {
    ($name:ident, ($($mutable:tt)?)) => {
        pub fn $name(
            &$($mutable)? self,
            node: &mut impl FnMut(&$($mutable)? Exact),
            leaf: &mut impl FnMut(&$($mutable)? CheckedExpression),
        ) {
            node(self);
            match &$($mutable)? self.kind {
                ExactKind::Literal(_) => {}
                ExactKind::Leaf(value) => leaf(value),
                ExactKind::Negate(value) => value.$name(node, leaf),
                ExactKind::Binary(_, left, right) => {
                    left.$name(node, leaf);
                    right.$name(node, leaf);
                }
            }
        }
    };
}

impl Exact {
    pub fn range_at(&self, index: usize) -> Result<Option<Range>, ()> {
        self.domains
            .as_ref()
            .map_or(Ok(self.range), |ranges| ranges[index])
    }
    exact_visitor!(visit, ());
    exact_visitor!(visit_mut, (mut));

    pub fn leaves(&self, leaf: &mut impl FnMut(&CheckedExpression)) {
        self.visit(&mut |_| {}, leaf);
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
    !super::expression::has_float_source(expr, bindings)
        && super::expression::operand_type(expr, bindings).is_none_or(Type::is_integer)
}
fn error(location: Location) -> Diagnostic {
    location.error(
        ErrorKind::InvalidSource,
        crate::diagnostic!(
            "arith.width",
            "exact integer expression exceeds 128 bits; store an intermediate value or use as"
        ),
    )
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
            let range = if left.domains.is_some() || right.domains.is_some() {
                None
            } else {
                match (left.range, right.range) {
                    (Some(a), Some(b)) => a.binary(*operator, b).map_err(|_| error(location))?,
                    _ => None,
                }
            };
            (
                ExactKind::Binary(*operator, Box::new(left), Box::new(right)),
                range,
            )
        }
        _ => {
            let value = super::expression::expression(expr, None, bindings)?;
            if !value.ty.is_integer() {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    if matches!(value.ty, Type::Float(_)) {
                        crate::diagnostic!(
                            "arith.exact",
                            "integer arithmetic requires an integer operand"
                        )
                    } else {
                        crate::diagnostic!(
                            "expr.operator-types",
                            "integer arithmetic requires integer operands"
                        )
                    },
                ));
            }
            let (min, max) = domain::bounds(value.ty).unwrap();
            (
                ExactKind::Leaf(Box::new(value)),
                Some(Range::bounds(min, max)),
            )
        }
    };
    let domains = match &kind {
        ExactKind::Leaf(value) if value.ty.symbolic() => Some(Box::new(std::array::from_fn(|i| {
            let ty = domain::at(value.ty, i).ok_or(())?;
            Ok(ty.integer_bounds().map(|(lo, hi)| Range::bounds(lo, hi)))
        }))),
        ExactKind::Negate(value) if value.domains.is_some() => {
            Some(Box::new(std::array::from_fn(|i| {
                let range = value.range_at(i)?.map(|r| Range {
                    min: r.max.neg(),
                    max: r.min.neg(),
                });
                if range.is_some_and(|r| !r.fits()) {
                    Err(())
                } else {
                    Ok(range)
                }
            })))
        }
        ExactKind::Binary(op, a, b) if a.domains.is_some() || b.domains.is_some() => {
            Some(Box::new(std::array::from_fn(|i| {
                let range = match (a.range_at(i)?, b.range_at(i)?) {
                    (Some(a), Some(b)) => a.binary(*op, b)?,
                    _ => None,
                };
                if range.is_some_and(|r| !r.fits()) {
                    Err(())
                } else {
                    Ok(range)
                }
            })))
        }
        _ => None,
    };
    let range = if let Some(ranges) = &domains {
        if ranges.iter().all(Result::is_err) {
            return Err(error(location));
        }
        bindings.defer_rule(|i| ranges[i].is_err().then(|| error(location)));
        ranges
            .iter()
            .filter_map(|r| r.ok().flatten())
            .reduce(|a, b| Range {
                min: a.min.min(b.min),
                max: a.max.max(b.max),
            })
    } else {
        if range.is_some_and(|r| !r.fits()) {
            return Err(error(location));
        }
        range
    };
    Ok(Exact {
        arithmetic: is_operator(expr)
            || matches!(expr.kind, ExpressionKind::Integer { negative: true, .. }),
        kind,
        range,
        domains,
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
            crate::diagnostic!(
                "arith.div-zero-always",
                "integer expression always divides by zero and requires or"
            ),
        ));
    }
    if !handled && exact.domains.is_some() {
        bindings.defer_rule(|i| {
            exact.range_at(i).is_ok_and(|r| r.is_none()).then(|| {
                exact.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "arith.div-zero-always",
                        "integer expression always divides by zero and requires or"
                    ),
                )
            })
        });
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
                crate::diagnostic!("conv.join", "integer operands have no common storage type"),
            ));
        }
        if let Some(ty) = known {
            bindings.defer_rule(|i| {
                domain::at(ty, i).is_none().then(|| {
                    expr.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "conv.join",
                            "integer operands have no common storage type"
                        ),
                    )
                })
            });
        }
        known.unwrap_or(Type::Integer(IntegerType::I64))
    };
    if ty == Type::Exact {
        return Ok(CheckedExpression {
            ty,
            value: Value::Exact(Box::new(exact)),
        });
    }
    if ty.is_float() {
        let possible = domain::any(|i| {
            let Some(Type::Float(float)) = domain::at(ty, i) else {
                return false;
            };
            exact
                .range_at(i)
                .is_ok_and(|r| r.is_none_or(|r| r.exact_float(float == FloatType::F32)))
        });
        if !possible {
            return Err(exact.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "arith.store-float",
                    "integer range is not exactly representable; use as"
                ),
            ));
        }
        bindings.defer_rule(|i| {
            let Type::Float(float) = domain::at(ty, i)? else {
                return None;
            };
            let range = exact.range_at(i).ok()?;
            range
                .is_some_and(|r| !r.exact_float(float == FloatType::F32))
                .then(|| {
                    exact.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "arith.store-float",
                            "integer range is not exactly representable; use as"
                        ),
                    )
                })
        });
    } else if !ty.is_integer() {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "type.expected",
                "integer arithmetic requires a numeric destination"
            ),
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
pub(super) fn range_at(value: &CheckedExpression, index: usize) -> Option<Range> {
    match &value.value {
        Value::Exact(e) => e.range_at(index).ok().flatten(),
        _ => domain::at(value.ty, index)?
            .integer_bounds()
            .map(|(a, b)| Range::bounds(a, b)),
    }
}
pub(super) fn value_range(expr: &CheckedExpression) -> Option<Range> {
    match &expr.value {
        Value::Exact(e) => e.range,
        _ => domain::bounds(expr.ty).map(|(a, b)| Range::bounds(a, b)),
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
    if from.symbolic() || to.symbolic() {
        return domain::judge(from, to, widening).possible();
    }
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
    if a.symbolic() || b.symbolic() {
        return domain::from_types(std::array::from_fn(|i| {
            join(domain::at(a, i)?, domain::at(b, i)?)
        }));
    }
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
                crate::diagnostic!(
                    "expr.cast-wrap-handler-syntax",
                    "integer as% does not accept or"
                ),
            ));
        }
        if !target.is_integer() {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("expr.as-trunc", "as% requires an integer target"),
            ));
        }
        return Ok(CheckedExpression {
            ty: target,
            value: Value::Truncate(Box::new(operand)),
        });
    }
    if !matches!(
        target,
        Type::Integer(_)
            | Type::Float(_)
            | Type::GenericInt
            | Type::GenericFloat
            | Type::Numeric(_)
            | Type::Enum(_)
            | Type::Char
    ) {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("expr.as", "invalid exact integer conversion"),
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
    super::expression::casts::require_handler(&operand, target, safe, location, bindings)?;
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
    if super::constants::depends_on_generic(a) || super::constants::depends_on_generic(b) {
        domain::uniform(|i| compare_ranges(range_at(a, i)?, range_at(b, i)?, op))
    } else {
        compare_ranges(value_range(a)?, value_range(b)?, op)
    }
}
fn compare_ranges(a: Range, b: Range, op: crate::ast::ComparisonOperator) -> Option<bool> {
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
