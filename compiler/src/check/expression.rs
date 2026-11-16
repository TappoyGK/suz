use crate::check::generic::domain;
use crate::check::scope::Scope;
mod blocks;
mod branches;
pub(in crate::check) mod casts;
mod operators;
mod slices;
pub(crate) mod structs;

use super::ir::{CheckedExpression, Value};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, FloatType, IntegerType, Location, Type},
};

// Find a known type through expressions that preserve their operand type.
// Numeric literals receive a type from the other operand before defaulting.
// The comparison's bool result never supplies the operand type.
pub(super) fn operand_type(expr: &Expression, bindings: &Scope) -> Option<Type> {
    let key = std::ptr::from_ref(expr) as usize;
    if bindings.hint_depth.get() > 0
        && let Some(ty) = bindings.type_hints.borrow().get(&key)
    {
        return *ty;
    }
    let ty = operand_type_inner(expr, bindings);
    if bindings.hint_depth.get() > 0 {
        bindings.type_hints.borrow_mut().insert(key, ty);
    }
    ty
}

fn operand_type_inner(expr: &Expression, bindings: &Scope) -> Option<Type> {
    #[cfg(test)]
    super::generic::work();
    match &expr.kind {
        ExpressionKind::Slice { index, .. } => Some(if *index { Type::Char } else { Type::Text }),
        ExpressionKind::Template(_) => Some(Type::Symbol),
        ExpressionKind::Struct { id, .. } => Some(Type::Struct(*id)),
        ExpressionKind::Field { target, name } => {
            let Type::Struct(id) = operand_type(target, bindings)? else {
                return None;
            };
            bindings.structs[id]
                .fields
                .iter()
                .find(|field| &field.name == name)
                .map(|field| field.ty)
        }
        ExpressionKind::Variant { enum_name, .. } => bindings
            .enums
            .iter()
            .find(|e| &e.name == enum_name)
            .map(|e| Type::Enum(e.ty)),
        ExpressionKind::Void => Some(Type::Void),
        ExpressionKind::Call { name, arguments } => {
            super::generic::call_operand_type(name, arguments, bindings)
        }
        ExpressionKind::Block { statements, value } => {
            blocks::operand_type(statements, value, bindings)
        }
        ExpressionKind::Match { arms, .. } => {
            joined_type(arms.iter().map(|arm| operand_type(&arm.body, bindings)))
        }
        ExpressionKind::Cast { target, .. } => Some(bindings.resolve_type(*target)),
        ExpressionKind::If {
            branches,
            otherwise,
        } => joined_type(
            branches
                .iter()
                .map(|(_, value)| operand_type(value, bindings))
                .chain(std::iter::once(operand_type(otherwise, bindings))),
        ),
        ExpressionKind::Char(_) => Some(Type::Char),
        ExpressionKind::Integer { .. } | ExpressionKind::Float(_) => None,
        ExpressionKind::Name(name) => bindings.get(name).map(|binding| binding.ty),
        ExpressionKind::ArithmeticHandler { operand: inner, .. }
        | ExpressionKind::Group(inner)
        | ExpressionKind::BitNot(inner)
        | ExpressionKind::Negate(inner) => operand_type(inner, bindings),
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } => {
            let a = operand_type(left, bindings);
            let b = operand_type(right, bindings);
            if operator.is_shift() {
                return a;
            }
            if operator.is_arithmetic() {
                let floats = [a, b].into_iter().flatten().filter(|t| t.is_float());
                if let Some(ty) = joined_type(floats.map(Some)) {
                    return Some(ty);
                }
                if has_float_source(left, bindings) || has_float_source(right, bindings) {
                    return None;
                }
            }
            match (a, b) {
                (Some(a), Some(b)) => Some(joined_or_first(a, b)),
                _ => a.or(b),
            }
        }

        ExpressionKind::Bool(_)
        | ExpressionKind::Not(_)
        | ExpressionKind::Logical { .. }
        | ExpressionKind::Comparison { .. } => Some(Type::Bool),
    }
}

// Float literals and generic float results select floating-point arithmetic.
// Only result-producing operands participate; conditions and cast sources do not.
pub(super) fn has_float_source(expr: &Expression, bindings: &Scope) -> bool {
    let key = std::ptr::from_ref(expr) as usize;
    if bindings.hint_depth.get() > 0
        && let Some(found) = bindings.float_hints.borrow().get(&key)
    {
        return *found;
    }
    let found = has_float_source_inner(expr, bindings);
    if bindings.hint_depth.get() > 0 {
        bindings.float_hints.borrow_mut().insert(key, found);
    }
    found
}

fn has_float_source_inner(expr: &Expression, bindings: &Scope) -> bool {
    match &expr.kind {
        ExpressionKind::Call { name, .. } => bindings
            .functions
            .get(name)
            .is_some_and(|f| f.result == Type::GenericFloat),
        ExpressionKind::Match { arms, .. } => {
            arms.iter().any(|arm| has_float_source(&arm.body, bindings))
        }
        ExpressionKind::Block { value, .. } => has_float_source(value, bindings),
        ExpressionKind::Float(_) => true,
        ExpressionKind::ArithmeticHandler { operand: inner, .. }
        | ExpressionKind::Group(inner)
        | ExpressionKind::BitNot(inner)
        | ExpressionKind::Negate(inner) => has_float_source(inner, bindings),
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } => {
            has_float_source(left, bindings)
                || (!operator.is_shift() && has_float_source(right, bindings))
        }
        ExpressionKind::If {
            branches,
            otherwise,
        } => {
            branches
                .iter()
                .any(|(_, value)| has_float_source(value, bindings))
                || has_float_source(otherwise, bindings)
        }
        _ => false,
    }
}
fn default_type(expr: &Expression, bindings: &Scope) -> Type {
    if has_float_source(expr, bindings) {
        Type::Float(FloatType::F64)
    } else {
        Type::Integer(IntegerType::I64)
    }
}
pub(in crate::check) fn float_literal(
    text: &str,
    ty: FloatType,
    location: Location,
) -> Result<CheckedExpression, Diagnostic> {
    let invalid = || {
        location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("lit.float-range", "floating-point literal is out of range"),
        )
    };
    let bits = float_bits(text, ty).ok_or_else(invalid)?;
    Ok(CheckedExpression {
        ty: Type::Float(ty),
        value: Value::Float(bits),
    })
}

pub(in crate::check) fn float_bits(text: &str, ty: FloatType) -> Option<u64> {
    let special = matches!(text, "nan" | "-nan" | "inf" | "-inf");
    let text = text.replace('_', "");
    let bits = match ty {
        FloatType::F32 => {
            let value = text.parse::<f32>().ok()?;
            if value.is_infinite() && !special {
                return None;
            }
            u64::from(value.to_bits())
        }
        FloatType::F64 => {
            let value = text.parse::<f64>().ok()?;
            if value.is_infinite() && !special {
                return None;
            }
            value.to_bits()
        }
    };
    Some(bits)
}

// Hints are shared only while checking one live AST expression. Clear them at
// the boundary so pointer keys cannot outlive their expressions or bindings.
struct HintSession<'a>(&'a Scope);
impl<'a> HintSession<'a> {
    fn new(scope: &'a Scope) -> Self {
        if scope.hint_depth.get() == 0 {
            scope.type_hints.borrow_mut().clear();
            scope.float_hints.borrow_mut().clear();
        }
        scope.hint_depth.set(scope.hint_depth.get() + 1);
        Self(scope)
    }
}
impl Drop for HintSession<'_> {
    fn drop(&mut self) {
        self.0.hint_depth.set(self.0.hint_depth.get() - 1);
        if self.0.hint_depth.get() == 0 {
            self.0.type_hints.borrow_mut().clear();
            self.0.float_hints.borrow_mut().clear();
        }
    }
}

pub(super) fn expression(
    expr: &Expression,
    expected: Option<Type>,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let _hints = HintSession::new(bindings);
    let checked = expression_inner(expr, expected, bindings, false, None)?;
    super::text::register_constants(&checked, bindings);
    Ok(checked)
}

// Numeric widening is opt-in at the contexts listed in conv.widen-places.
pub(super) fn coerced(
    expr: &Expression,
    expected: Option<Type>,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let _hints = HintSession::new(bindings);
    let checked = expression_inner(expr, expected, bindings, true, None)?;
    super::text::register_constants(&checked, bindings);
    Ok(checked)
}

// This context belongs only to the argument boundary. Parentheses preserve
// that boundary; other subexpressions use their own ordinary checking context.
pub(super) fn generic_argument(
    expr: &Expression,
    expected: Type,
    family: super::generic::Family,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let _hints = HintSession::new(bindings);
    let checked = expression_inner(expr, Some(expected), bindings, true, Some(family))?;
    super::text::register_constants(&checked, bindings);
    Ok(checked)
}

fn expression_inner(
    expr: &Expression,
    required: Option<Type>,
    bindings: &Scope,
    widen: bool,
    argument: Option<super::generic::Family>,
) -> Result<CheckedExpression, Diagnostic> {
    // Infer an incompatible family independently so that the outer parameter
    // owns the conversion diagnostic, including for nested generic calls.
    let inference_hint = if argument == Some(super::generic::Family::Int)
        && (has_float_source(expr, bindings)
            || operand_type(expr, bindings).is_some_and(Type::is_float))
    {
        None
    } else {
        required
    };
    let checked = if super::exact::is_operator(expr) && super::exact::integer(expr, bindings) {
        super::exact::check(expr, inference_hint, bindings, false)?
    } else {
        match &expr.kind {
            ExpressionKind::Slice { .. } => slices::check(expr, bindings)?,
            ExpressionKind::Template(parts) => {
                super::text::template(parts, expr.location, bindings)?
            }
            ExpressionKind::Struct { .. } | ExpressionKind::Field { .. } => {
                structs::check(expr, bindings)?
            }
            ExpressionKind::Variant { enum_name, variant } => {
                let decl = bindings
                    .enums
                    .iter()
                    .find(|e| &e.name == enum_name)
                    .ok_or_else(|| {
                        expr.location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!("enum.access", "unknown enum type"),
                        )
                    })?;
                let value = decl
                    .variants
                    .iter()
                    .position(|name| name == variant)
                    .ok_or_else(|| {
                        expr.location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!("enum.access", "unknown enum variant"),
                        )
                    })?;
                CheckedExpression {
                    ty: Type::Enum(decl.ty),
                    value: Value::Integer {
                        value: value as i128,
                        minus: None,
                    },
                }
            }
            ExpressionKind::Void => CheckedExpression {
                ty: Type::Void,
                value: Value::Void,
            },
            ExpressionKind::Call { name, arguments } => {
                super::functions::call(expr, name, arguments, inference_hint, bindings)?
            }
            ExpressionKind::Block { statements, value } => {
                blocks::check(statements, value, inference_hint, bindings)?
            }
            ExpressionKind::ArithmeticHandler { .. }
            | ExpressionKind::BitNot(_)
            | ExpressionKind::Negate(_)
            | ExpressionKind::Binary { .. }
            | ExpressionKind::Not(_)
            | ExpressionKind::Logical { .. }
            | ExpressionKind::Comparison { .. } => {
                operators::check(expr, inference_hint, bindings)?
            }
            ExpressionKind::Cast { .. } => casts::check(expr, bindings)?,
            ExpressionKind::Float(text) if inference_hint.is_some_and(Type::symbolic) => {
                let ty = inference_hint.unwrap();
                if !ty.is_float() {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "type.expected",
                            "a floating-point literal requires a floating-point type"
                        ),
                    ));
                }
                symbolic_literal(text, false, ty, expr.location, bindings)?
            }
            ExpressionKind::Float(text) => {
                let ty = match inference_hint {
                    None => FloatType::F64,
                    Some(Type::Float(ty)) => ty,
                    _ => {
                        return Err(expr.location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!(
                                "type.expected",
                                "a floating-point literal requires a floating-point type"
                            ),
                        ));
                    }
                };
                float_literal(text, ty, expr.location)?
            }
            ExpressionKind::Match { .. } | ExpressionKind::If { .. } => {
                branches::check(expr, inference_hint, bindings)?
            }
            ExpressionKind::Integer { digits, negative }
                if inference_hint.is_some_and(Type::symbolic) =>
            {
                symbolic_literal(
                    digits,
                    *negative,
                    inference_hint.unwrap(),
                    expr.location,
                    bindings,
                )?
            }
            ExpressionKind::Integer { digits, negative } => {
                if let Some(Type::Float(ty)) = inference_hint {
                    let text = if *negative {
                        format!("-{digits}")
                    } else {
                        digits.clone()
                    };
                    return float_literal(&text, ty, expr.location);
                }
                let ty = match inference_hint {
                    Some(Type::Float(_)) => unreachable!(),
                    Some(
                        Type::Numeric(_)
                        | Type::GenericInt
                        | Type::GenericFloat
                        | Type::Exact
                        | Type::Char
                        | Type::Bool
                        | Type::Void
                        | Type::Enum(_)
                        | Type::Struct(_)
                        | Type::Symbol
                        | Type::Text,
                    ) => {
                        return Err(expr.location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!(
                                "type.expected",
                                "an integer literal requires a numeric type"
                            ),
                        ));
                    }
                    Some(Type::Integer(ty)) => ty,
                    None => IntegerType::I64,
                };
                integer_literal(digits, *negative, ty, expr.location)?
            }

            ExpressionKind::Char(value) => CheckedExpression {
                ty: Type::Char,
                value: Value::Integer {
                    value: i128::from(*value),
                    minus: None,
                },
            },
            ExpressionKind::Bool(value) => CheckedExpression {
                ty: Type::Bool,
                value: Value::Bool(*value),
            },
            ExpressionKind::Name(name) => {
                let binding = bindings.get(name).ok_or_else(|| {
                    expr.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("decl.local-scope", "binding '{name}' is not in scope"),
                    )
                })?;
                binding.constant.clone().unwrap_or(CheckedExpression {
                    ty: binding.ty,
                    value: Value::Local(binding.id),
                })
            }
            // Parentheses preserve the argument boundary; they do not create
            // a new expression context or an independent conversion.
            ExpressionKind::Group(inner) => {
                expression_inner(inner, required, bindings, widen, argument)?
            }
        }
    };

    let generic_result = matches!(&expr.kind, ExpressionKind::Call { name, .. }
        if bindings.functions.get(name).is_some_and(|f| f.result.generic()));
    if let Some(to) = required
        && (to.symbolic() || checked.ty.symbolic())
        && (domain::same(to, checked.ty) || (widen && super::exact::widening(checked.ty, to)))
    {
        bindings.defer_rule(|i| {
            let (from, to) = (domain::at(checked.ty, i)?, domain::at(to, i)?);
            (!(from == to || (widen && super::exact::widening(from, to)))).then(|| {
                conversion_error(expr.location, from, to, argument, generic_result, bindings)
            })
        });
    }

    if let Some(ty) = required
        && widen
        && ty != checked.ty
        && super::exact::widening(checked.ty, ty)
    {
        return Ok(CheckedExpression {
            ty,
            value: Value::Cast(Box::new(checked)),
        });
    }
    if required == Some(Type::Text) && checked.ty == Type::Symbol {
        return Ok(CheckedExpression {
            ty: Type::Text,
            value: Value::TextView(Box::new(checked)),
        });
    }
    if required.is_some_and(|ty| !domain::same(ty, checked.ty)) {
        return Err(conversion_error(
            expr.location,
            checked.ty,
            required.unwrap(),
            argument,
            generic_result,
            bindings,
        ));
    }
    Ok(checked)
}

fn conversion_error(
    location: Location,
    from: Type,
    to: Type,
    argument: Option<super::generic::Family>,
    generic_result: bool,
    bindings: &Scope,
) -> Diagnostic {
    if Type::is_numeric(from) && Type::is_numeric(to) && (argument.is_some() || generic_result) {
        let message = if argument == Some(super::generic::Family::Int) && from.is_float() {
            crate::diagnostic!(
                "num.fn-convert",
                "floating-point values cannot be passed to an int parameter"
            )
            .to_owned()
        } else {
            crate::diagnostic!(
                "num.fn-convert",
                "{} {} cannot be {} as {} without loss",
                if argument.is_some() {
                    "argument"
                } else {
                    "generic result"
                },
                bindings.type_name(from),
                if argument.is_some() {
                    "passed"
                } else {
                    "received"
                },
                bindings.type_name(to)
            )
        };
        return location.error(ErrorKind::InvalidSource, message);
    }
    location.error(
        ErrorKind::InvalidSource,
        crate::diagnostic!(
            "type.expected",
            "expected {}, found {}",
            bindings.type_name(to),
            bindings.type_name(from)
        ),
    )
}

pub(super) fn joined_type(types: impl Iterator<Item = Option<Type>>) -> Option<Type> {
    types.flatten().reduce(joined_or_first)
}

// Apply the fallback per substitution too: a missing common type must still
// reach the ordinary conversion judgment against the first operand's type.
fn joined_or_first(a: Type, b: Type) -> Type {
    if a.symbolic() || b.symbolic() {
        return domain::from_types(std::array::from_fn(|i| {
            let (a, b) = (domain::at(a, i)?, domain::at(b, i)?);
            Some(super::exact::join(a, b).unwrap_or(a))
        }))
        .unwrap_or(a);
    }
    super::exact::join(a, b).unwrap_or(a)
}

pub(in crate::check) fn integer_literal(
    digits: &str,
    negative: bool,
    ty: IntegerType,
    location: Location,
) -> Result<CheckedExpression, Diagnostic> {
    let value = digits
        .parse::<i128>()
        .ok()
        .and_then(|n| if negative { n.checked_neg() } else { Some(n) });
    let value = integer_literal_value(value, negative, ty, location)?;
    Ok(CheckedExpression {
        ty: Type::Integer(ty),
        value: Value::Integer {
            value,
            minus: negative.then_some(location),
        },
    })
}
fn integer_literal_value(
    value: Option<i128>,
    negative: bool,
    ty: IntegerType,
    location: Location,
) -> Result<i128, Diagnostic> {
    let fail = || {
        location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "lit.range",
                "integer literal is out of range for {}",
                ty.name()
            ),
        )
    };
    let value = value.ok_or_else(fail)?;
    let (min, max) = ty.bounds();
    if value < min || value > max {
        return Err(fail());
    }
    if negative && min == 0 {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "expr.operator-types",
                "integer negation requires a signed type"
            ),
        ));
    }
    Ok(value)
}

// A literal is checked once. Width-dependent range failures are deferred;
// the value keeps its symbolic type for the enclosing rule.
fn symbolic_literal(
    text: &str,
    negative: bool,
    ty: Type,
    location: Location,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    use domain::judge_one;
    if ty.is_integer() {
        let value = text
            .parse::<i128>()
            .ok()
            .and_then(|n| if negative { n.checked_neg() } else { Some(n) });
        if let Some(value) = value
            && judge_one(ty, |t| {
                let (lo, hi) = t.integer_bounds().unwrap();
                value >= lo && value <= hi && (!negative || lo < 0)
            })
            .possible()
        {
            bindings.defer_rule(|i| {
                let Type::Integer(ty) = domain::at(ty, i)? else {
                    return None;
                };
                integer_literal_value(Some(value), negative, ty, location).err()
            });
            return Ok(CheckedExpression {
                ty,
                value: Value::Integer {
                    value,
                    minus: negative.then_some(location),
                },
            });
        }
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("lit.range", "integer literal is out of range for int"),
        ));
    }
    if ty.is_float() {
        let text = if negative {
            format!("-{text}")
        } else {
            text.to_owned()
        };
        if domain::any(
            |i| matches!(domain::at(ty, i), Some(Type::Float(t)) if float_bits(&text, t).is_some()),
        ) {
            bindings.defer_rule(|i| {
                let Type::Float(ty) = domain::at(ty, i)? else {
                    return None;
                };
                float_literal(&text, ty, location).err()
            });
            return Ok(CheckedExpression {
                ty,
                value: Value::GenericFloatLiteral(text),
            });
        }
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "lit.float-range",
                "floating-point literal is out of range for float"
            ),
        ));
    }
    unreachable!("symbolic numeric type")
}
