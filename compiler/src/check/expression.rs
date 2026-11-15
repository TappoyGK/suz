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
                let floats = [a, b]
                    .into_iter()
                    .flatten()
                    .filter(|t| matches!(t, Type::Float(_)));
                if let Some(ty) = joined_type(floats.map(Some)) {
                    return Some(ty);
                }
                if has_float_literal(left, bindings) || has_float_literal(right, bindings) {
                    return None;
                }
            }
            match (a, b) {
                (Some(a), Some(b)) => Some(super::exact::join(a, b).unwrap_or(a)),
                _ => a.or(b),
            }
        }

        ExpressionKind::Bool(_)
        | ExpressionKind::Not(_)
        | ExpressionKind::Logical { .. }
        | ExpressionKind::Comparison { .. } => Some(Type::Bool),
    }
}

// Only result-producing operands participate; conditions and cast sources do not.
pub(super) fn has_float_literal(expr: &Expression, bindings: &Scope) -> bool {
    let key = std::ptr::from_ref(expr) as usize;
    if bindings.hint_depth.get() > 0
        && let Some(found) = bindings.float_hints.borrow().get(&key)
    {
        return *found;
    }
    let found = has_float_literal_inner(expr, bindings);
    if bindings.hint_depth.get() > 0 {
        bindings.float_hints.borrow_mut().insert(key, found);
    }
    found
}

fn has_float_literal_inner(expr: &Expression, bindings: &Scope) -> bool {
    match &expr.kind {
        ExpressionKind::Call { name, .. } => bindings
            .functions
            .get(name)
            .is_some_and(|f| f.result == Type::GenericFloat),
        ExpressionKind::Match { arms, .. } => arms
            .iter()
            .any(|arm| has_float_literal(&arm.body, bindings)),
        ExpressionKind::Block { value, .. } => has_float_literal(value, bindings),
        ExpressionKind::Float(_) => true,
        ExpressionKind::ArithmeticHandler { operand: inner, .. }
        | ExpressionKind::Group(inner)
        | ExpressionKind::BitNot(inner)
        | ExpressionKind::Negate(inner) => has_float_literal(inner, bindings),
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } => {
            has_float_literal(left, bindings)
                || (!operator.is_shift() && has_float_literal(right, bindings))
        }
        ExpressionKind::If {
            branches,
            otherwise,
        } => {
            branches
                .iter()
                .any(|(_, value)| has_float_literal(value, bindings))
                || has_float_literal(otherwise, bindings)
        }
        _ => false,
    }
}
fn default_type(expr: &Expression, bindings: &Scope) -> Type {
    if has_float_literal(expr, bindings) {
        Type::Float(FloatType::F64)
    } else {
        Type::Integer(IntegerType::I64)
    }
}
fn float_literal(
    text: &str,
    ty: FloatType,
    location: Location,
) -> Result<CheckedExpression, Diagnostic> {
    let invalid = || {
        location.error(
            ErrorKind::InvalidSource,
            "floating-point literal is out of range [lit.float-range]",
        )
    };
    let special = matches!(text, "nan" | "-nan" | "inf" | "-inf");
    let text = text.replace('_', "");
    let bits = match ty {
        FloatType::F32 => {
            let value = text.parse::<f32>().map_err(|_| invalid())?;
            if value.is_infinite() && !special {
                return Err(invalid());
            }
            u64::from(value.to_bits())
        }
        FloatType::F64 => {
            let value = text.parse::<f64>().map_err(|_| invalid())?;
            if value.is_infinite() && !special {
                return Err(invalid());
            }
            value.to_bits()
        }
    };
    Ok(CheckedExpression {
        ty: Type::Float(ty),
        value: Value::Float(bits),
    })
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
    let checked = expression_inner(expr, expected, bindings, false)?;
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
    let checked = expression_inner(expr, expected, bindings, true)?;
    super::text::register_constants(&checked, bindings);
    Ok(checked)
}

fn expression_inner(
    expr: &Expression,
    expected: Option<Type>,
    bindings: &Scope,
    widen: bool,
) -> Result<CheckedExpression, Diagnostic> {
    let checked = if super::exact::is_operator(expr) && super::exact::integer(expr, bindings) {
        super::exact::check(expr, expected, bindings, false)?
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
                        expr.location
                            .error(ErrorKind::InvalidSource, "unknown enum type [enum.access]")
                    })?;
                let value = decl
                    .variants
                    .iter()
                    .position(|name| name == variant)
                    .ok_or_else(|| {
                        expr.location.error(
                            ErrorKind::InvalidSource,
                            "unknown enum variant [enum.access]",
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
                super::functions::call(expr, name, arguments, expected, bindings)?
            }
            ExpressionKind::Block { statements, value } => {
                blocks::check(statements, value, expected, bindings)?
            }
            ExpressionKind::ArithmeticHandler { .. }
            | ExpressionKind::BitNot(_)
            | ExpressionKind::Negate(_)
            | ExpressionKind::Binary { .. }
            | ExpressionKind::Not(_)
            | ExpressionKind::Logical { .. }
            | ExpressionKind::Comparison { .. } => operators::check(expr, expected, bindings)?,
            ExpressionKind::Cast { .. } => casts::check(expr, bindings)?,
            ExpressionKind::Float(text) => {
                let ty = match expected {
                    None => FloatType::F64,
                    Some(Type::Float(ty)) => ty,
                    _ => return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        "a floating-point literal requires a floating-point type [type.expected]",
                    )),
                };
                float_literal(text, ty, expr.location)?
            }
            ExpressionKind::Match { .. } | ExpressionKind::If { .. } => {
                branches::check(expr, expected, bindings)?
            }
            ExpressionKind::Integer { digits, negative } => {
                if let Some(Type::Float(ty)) = expected {
                    let text = if *negative {
                        format!("-{digits}")
                    } else {
                        digits.clone()
                    };
                    return float_literal(&text, ty, expr.location);
                }
                let ty = match expected {
                    Some(Type::Float(_)) => unreachable!(),
                    Some(
                        Type::GenericInt
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
                            "an integer literal requires a numeric type [type.expected]",
                        ));
                    }
                    Some(Type::Integer(ty)) => ty,
                    None => IntegerType::I64,
                };
                let fail = || {
                    expr.location.error(
                        ErrorKind::InvalidSource,
                        format!(
                            "integer literal is out of range for {} [lit.range]",
                            ty.name()
                        ),
                    )
                };
                let magnitude = digits.parse::<i128>().map_err(|_| fail())?;
                let value = if *negative { -magnitude } else { magnitude };
                let (min, max) = ty.bounds();
                if value < min || value > max {
                    return Err(fail());
                }
                if *negative && min == 0 {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        "integer negation requires a signed type [expr.operator-types]",
                    ));
                }
                CheckedExpression {
                    ty: Type::Integer(ty),
                    value: Value::Integer {
                        value,
                        minus: negative.then_some(expr.location),
                    },
                }
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
                        format!("binding '{name}' is not in scope [decl.local-scope]"),
                    )
                })?;
                binding.constant.clone().unwrap_or(CheckedExpression {
                    ty: binding.ty,
                    value: Value::Local(binding.id),
                })
            }
            ExpressionKind::Group(inner) => expression_inner(inner, expected, bindings, widen)?,
        }
    };
    if let Some(ty) = expected
        && widen
        && ty != checked.ty
        && super::exact::widening(checked.ty, ty)
    {
        return Ok(CheckedExpression {
            ty,
            value: Value::Cast(Box::new(checked)),
        });
    }
    if expected == Some(Type::Text) && checked.ty == Type::Symbol {
        return Ok(CheckedExpression {
            ty: Type::Text,
            value: Value::TextView(Box::new(checked)),
        });
    }
    if expected.is_some_and(|ty| ty != checked.ty) {
        if let ExpressionKind::Call { name, .. } = &expr.kind
            && bindings.functions.get(name).is_some_and(|f| f.is_generic())
        {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                format!(
                    "generic result {} cannot be received as {} without loss [num.fn-convert]",
                    bindings.type_name(checked.ty),
                    bindings.type_name(expected.unwrap())
                ),
            ));
        }
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            format!(
                "expected {}, found {} [type.expected]",
                bindings.type_name(expected.unwrap()),
                bindings.type_name(checked.ty)
            ),
        ));
    }
    Ok(checked)
}

pub(super) fn joined_type(types: impl Iterator<Item = Option<Type>>) -> Option<Type> {
    types
        .flatten()
        .reduce(|a, b| super::exact::join(a, b).unwrap_or(a))
}

// Track only whether a type depends on template parameters; source ASTs are
// never copied into bindings. Concrete annotations terminate this dependence.
pub(super) fn generic_dependent(expr: &Expression, scope: &Scope) -> bool {
    if !scope.in_generic() {
        return false;
    }
    match &expr.kind {
        ExpressionKind::Name(name) => scope.get(name).is_some_and(|b| b.generic_dependent),
        ExpressionKind::Call { name, .. } => scope
            .functions
            .get(name)
            .is_some_and(|f| f.result.generic()),
        ExpressionKind::Cast { target, .. } => target.generic(),
        ExpressionKind::Group(e)
        | ExpressionKind::Negate(e)
        | ExpressionKind::BitNot(e)
        | ExpressionKind::ArithmeticHandler { operand: e, .. } => generic_dependent(e, scope),
        ExpressionKind::Binary { left, right, .. } => {
            generic_dependent(left, scope) || generic_dependent(right, scope)
        }
        ExpressionKind::If {
            branches,
            otherwise,
        } => {
            branches.iter().any(|(_, e)| generic_dependent(e, scope))
                || generic_dependent(otherwise, scope)
        }
        ExpressionKind::Match { arms, .. } => {
            arms.iter().any(|a| generic_dependent(&a.body, scope))
        }
        // Block-local bindings are checked by the ordinary checker. Conservatively
        // retain dependence when a block is used as a numeric cast operand.
        ExpressionKind::Block { .. } => true,
        _ => false,
    }
}
