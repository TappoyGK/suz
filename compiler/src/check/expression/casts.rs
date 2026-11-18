//! 型変換の可否と失敗の可能性を調べ、必要なハンドラーを検査する。文字列と構造体の変換も扱う。
//!
//! 利用元: [`crate::check::expression`]。
//! 主な対象規則: `[const.cast-check]`、`[expr.as]`、`[expr.as-trunc]`、`[expr.cast-handler]`、`[expr.cast-wrap-handler-syntax]`、`[num.fn-cast]`。

use super::super::constants::{constant_float, constant_integer, float_to_integer};
use super::super::ir::{CheckedExpression, Value};
use crate::ast::{FailureHandler, FloatType};
use crate::check::generic::domain;
use crate::check::scope::Scope;
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, Type},
};

pub(in crate::check) fn check(
    expr: &Expression,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    Ok(match &expr.kind {
        ExpressionKind::Cast {
            wrap,
            operand,
            target: annotation,
            fallback,
        } => {
            let target = bindings.resolve_type(*annotation);
            if *wrap {
                let allowed = |ty| ty != Type::Integer(crate::ast::IntegerType::Len);
                let error = || {
                    expr.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("expr.as-trunc", "len_t cannot be the target of as%"),
                    )
                };
                match domain::judge_one(target, allowed) {
                    domain::Judgment::Invalid => return Err(error()),
                    domain::Judgment::Deferred => {
                        bindings.defer_rule(|i| (!allowed(domain::at(target, i)?)).then(error))
                    }
                    domain::Judgment::Valid => {}
                }
            }
            let operand = super::super::exact::use_value(operand, bindings)?;
            if operand.ty == Type::CPointer || target == Type::CPointer {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("ffi.pointer-operations", "pointer values cannot be cast"),
                ));
            }
            if matches!(operand.ty, Type::CInteger(_)) || matches!(target, Type::CInteger(_)) {
                return crate::check::ffi::cast(
                    operand,
                    target,
                    *wrap,
                    fallback,
                    expr.location,
                    bindings,
                );
            }
            if operand.ty == Type::Exact {
                return super::super::exact::cast(
                    operand,
                    target,
                    *wrap,
                    fallback,
                    expr.location,
                    bindings,
                );
            }
            if !*wrap && operand.ty == Type::Text && target == Type::Symbol {
                if fallback.is_some() {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("text.to-symbol", "text interning does not accept or"),
                    ));
                }
                let constant = super::super::constants::evaluate(&operand)
                    .ok()
                    .flatten()
                    .and_then(|value| {
                        if let Value::Text { text } = value.value {
                            Some(text)
                        } else {
                            None
                        }
                    });
                return Ok(CheckedExpression {
                    ty: Type::Symbol,
                    value: Value::Intern {
                        constant,
                        operand: Box::new(operand),
                        location: expr.location,
                    },
                });
            }
            if operand.ty.is_text() {
                let valid = if *wrap {
                    target.is_integer()
                } else {
                    matches!(
                        target,
                        Type::Integer(_)
                            | Type::Float(_)
                            | Type::GenericInt
                            | Type::GenericFloat
                            | Type::Numeric(_)
                            | Type::Bool
                            | Type::Char
                            | Type::Struct(_)
                    )
                };
                if !valid
                    || bindings.contains_function(target)
                    || crate::check::ffi::contains_c_type(target, bindings)
                {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        if *wrap {
                            crate::diagnostic!(
                                "symbol.cast-wrap",
                                "{} as% requires an integer target",
                                operand.ty.name()
                            )
                        } else {
                            crate::diagnostic!(
                                "symbol.cast",
                                "unsupported {} conversion target",
                                operand.ty.name()
                            )
                        },
                    ));
                }
                if fallback.is_none() {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "symbol.cast-handler",
                            "{} conversion requires or",
                            operand.ty.name()
                        ),
                    ));
                }
            } else if *wrap {
                if fallback.is_some() {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "expr.cast-wrap-handler-syntax",
                            "integer as% does not accept or"
                        ),
                    ));
                }
                if !(operand.ty.is_integer() && target.is_integer()) {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "expr.as-trunc",
                            "as% requires integer source and target types"
                        ),
                    ));
                }
                return Ok(CheckedExpression {
                    ty: target,
                    value: Value::Truncate(Box::new(operand)),
                });
            }
            let validity = domain::judge(operand.ty, target, |from, to| {
                conversion_safety(from, to).is_some()
            });
            if !validity.possible() {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("expr.as", "invalid source and target types for as"),
                ));
            }
            if validity == domain::Judgment::Deferred {
                bindings.defer_rule(|i| {
                    conversion_safety(domain::at(operand.ty, i)?, domain::at(target, i)?)
                        .is_none()
                        .then(|| {
                            expr.location.error(
                                ErrorKind::InvalidSource,
                                crate::diagnostic!(
                                    "expr.as",
                                    "invalid source and target types for as"
                                ),
                            )
                        })
                });
            }
            if fallback.is_none() && !operand.ty.is_text() {
                let safe = conversion_safety(operand.ty, target).unwrap_or(false);
                require_handler(&operand, target, safe, expr.location, bindings)?;
            }
            if let Some(FailureHandler::Return { value, .. }) = fallback {
                let return_value = crate::check::functions::return_value(value, bindings)?;
                let checked = CheckedExpression {
                    ty: target,
                    value: Value::CastReturn {
                        location: expr.location,
                        wrap: *wrap,
                        operand: Box::new(operand),
                        return_value: Box::new(return_value),
                    },
                };
                return Ok(checked);
            }
            let fallback = match fallback {
                Some(FailureHandler::Ignore(ignore)) => {
                    return Err(crate::check::errors::invalid_handler(ignore.location));
                }
                Some(FailureHandler::Errors(handler)) => {
                    return Err(crate::check::errors::invalid_handler(handler.location));
                }
                Some(FailureHandler::Fail { location, .. }) => {
                    return Err(crate::check::errors::invalid_handler(*location));
                }
                Some(FailureHandler::Value(value)) => {
                    Some(super::coerced(value, Some(target), bindings)?)
                }
                None => None,
                _ => unreachable!(),
            };
            if operand.ty.is_text() {
                let constant_struct = if let Type::Struct(id) = target
                    && super::super::constants::is_constant(fallback.as_ref().unwrap())
                {
                    // 選択されない定数オペランド内のエラーは、定数評価で扱う。
                    super::super::constants::evaluate(&operand)
                        .ok()
                        .flatten()
                        .and_then(|value| {
                            let Value::Text { text, .. } = value.value else {
                                return None;
                            };
                            super::super::text_structure::parse(&text, id, bindings).map(Box::new)
                        })
                } else {
                    None
                };
                if let Some(value) = &constant_struct {
                    debug_assert!(
                        !super::super::constants::depends_on_generic(value),
                        "parsed structure caches must contain only concrete values"
                    );
                }
                return Ok(CheckedExpression {
                    ty: target,
                    value: Value::CheckedCast {
                        location: expr.location,
                        constant_struct,
                        wrap: *wrap,
                        operand: Box::new(operand),
                        fallback: Box::new(fallback.unwrap()),
                    },
                });
            }
            let value = if let Some(fallback) = fallback {
                Value::CheckedCast {
                    location: expr.location,
                    constant_struct: None,
                    wrap: false,
                    operand: Box::new(operand),
                    fallback: Box::new(fallback),
                }
            } else {
                Value::Cast(Box::new(operand))
            };
            CheckedExpression { ty: target, value }
        }

        _ => unreachable!(),
    })
}

// [expr.cast-risk]、[arith.cast]、[const.cast-check]。通常検査と汎用検査で同じ分類を使い、Scopeは変更しない。
// スカラー変換の規則は通常の検査と記号的な検査で共有する。
fn conversion_safety(from: Type, to: Type) -> Option<bool> {
    if matches!(from, Type::Enum(e) if e.is_error) || matches!(to, Type::Enum(e) if e.is_error) {
        return None;
    }
    Some(match (from, to) {
        (Type::Symbol | Type::Text, _) => false,
        (Type::Char, Type::Integer(_)) | (Type::Integer(_), Type::Char) => {
            let (lo, hi) = from.integer_bounds().unwrap();
            crate::numeric::Range::bounds(lo, hi).fits_type(to)
        }
        (Type::Integer(a), Type::Integer(b)) => {
            let (lo, hi) = a.bounds();
            let (min, max) = b.bounds();
            lo >= min && hi <= max
        }
        (Type::Enum(a), Type::Integer(b)) => a.count as i128 - 1 <= b.bounds().1,
        (Type::Integer(_), Type::Enum(_)) | (Type::Float(_), Type::Integer(_)) => false,
        (Type::Integer(_), Type::Float(_)) => true,
        (Type::Float(a), Type::Float(b)) => a == b || a == FloatType::F32,
        _ => return None,
    })
}

#[derive(Clone, Copy)]
pub(in crate::check) enum CastRisk {
    Safe,
    Dynamic,
    ConstantOutOfRange,
}
impl CastRisk {
    pub(in crate::check) fn require_handler_for(
        self,
        location: crate::ast::Location,
        generic: bool,
    ) -> Result<(), Diagnostic> {
        if generic && !matches!(self, Self::Safe) {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "num.fn-cast",
                    "this conversion can fail for a generic numeric type and requires or"
                ),
            ));
        }
        self.require_handler(location)
    }
    pub(in crate::check) fn require_handler(
        self,
        location: crate::ast::Location,
    ) -> Result<(), Diagnostic> {
        match self {
            Self::Safe => Ok(()),
            Self::Dynamic => Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("expr.cast-handler", "this cast requires or"),
            )),
            Self::ConstantOutOfRange => Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "const.cast-check",
                    "constant cast is out of range and requires or"
                ),
            )),
        }
    }
}
pub(in crate::check) fn require_handler(
    operand: &CheckedExpression,
    target: Type,
    type_safe: bool,
    location: crate::ast::Location,
    scope: &Scope,
) -> Result<(), Diagnostic> {
    if type_safe {
        return Ok(());
    }
    let dependent = target.symbolic() || super::super::constants::depends_on_generic(operand);
    cast_risk(operand, target, scope, dependent)?
        .require_handler_for(location, scope.generic_check() && dependent)
}

fn cast_risk(
    operand: &CheckedExpression,
    target: Type,
    bindings: &Scope,
    dependent: bool,
) -> Result<CastRisk, Diagnostic> {
    if dependent {
        // [num.fn-cast] 安全性はすべての型の代入で必要であり、通常の保留する判定とは異なる。
        let safe = domain::try_all(|i| {
            if let (Some(from), Some(to)) = (domain::at(operand.ty, i), domain::at(target, i)) {
                let mut scalar = match super::super::constants::generic::at(operand, i, bindings) {
                    Ok(Some(value)) => value,
                    Ok(None) => CheckedExpression {
                        ty: from,
                        value: operand.value.clone(),
                    },
                    Err(_) => return Ok(false),
                };
                if let Value::Exact(e) = &mut scalar.value {
                    e.range = e.range_at(i).ok().flatten();
                    e.domains = None;
                }
                let safe = if from == Type::Exact {
                    super::super::exact::range_at(operand, i)
                        .is_some_and(|r| matches!(to, Type::Float(_)) || r.fits_type(to))
                } else {
                    conversion_safety(from, to).unwrap_or(false)
                };
                if !matches!(scalar_cast_risk(&scalar, to, safe)?, CastRisk::Safe) {
                    return Ok(false);
                }
            }
            Ok::<_, Diagnostic>(true)
        })?;
        return Ok(if safe {
            CastRisk::Safe
        } else {
            CastRisk::Dynamic
        });
    }
    scalar_cast_risk(operand, target, false)
}

fn scalar_cast_risk(
    operand: &CheckedExpression,
    target: Type,
    type_safe: bool,
) -> Result<CastRisk, Diagnostic> {
    if type_safe {
        return Ok(CastRisk::Safe);
    }
    let constant_fits = if operand.ty == Type::Exact {
        super::super::exact::constant(operand)
            .map(|n| crate::numeric::Range::point(n).fits_type(target))
    } else if matches!(operand.ty, Type::Float(_)) {
        constant_float(operand)?.map(|n| match target {
            Type::Integer(to) => float_to_integer(n, to).is_some(),
            Type::Float(FloatType::F32) => !n.is_finite() || (n as f32).is_finite(),
            _ => unreachable!("cast target validated before numeric risk classification"),
        })
    } else {
        constant_integer(operand)?.map(|n| {
            crate::numeric::Range::point(crate::numeric::Int::from_i128(n)).fits_type(target)
        })
    };
    Ok(match constant_fits {
        Some(true) => CastRisk::Safe,
        Some(false) => CastRisk::ConstantOutOfRange,
        None => CastRisk::Dynamic,
    })
}
