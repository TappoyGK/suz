mod policy;
mod validation;

use super::{
    expression,
    scope::{Scope, Signature},
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, FloatType, IntegerType, Location, Type},
};
pub(super) use policy::{comparison_warning, validate_cast};
use std::collections::{BTreeMap, VecDeque};
pub(super) use validation::validate;

#[derive(Clone)]
pub(super) struct Instance {
    pub function: usize,
    pub types: [Option<Type>; 2],
    pub id: usize,
    pub call: Location,
}
#[derive(Default, Clone)]
pub(super) struct Instances {
    ids: BTreeMap<(usize, Option<IntegerType>, Option<FloatType>), usize>,
    pub queue: VecDeque<Instance>,
    pub next_id: usize,
}
impl Instances {
    pub fn request(&mut self, function: usize, types: [Option<Type>; 2], call: Location) -> usize {
        let key = (
            function,
            types[0].and_then(|t| {
                if let Type::Integer(t) = t {
                    Some(t)
                } else {
                    None
                }
            }),
            types[1].and_then(|t| {
                if let Type::Float(t) = t {
                    Some(t)
                } else {
                    None
                }
            }),
        );
        if let Some(id) = self.ids.get(&key) {
            return *id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.ids.insert(key, id);
        self.queue.push_back(Instance {
            function,
            types,
            id,
            call,
        });
        id
    }
}

pub(super) fn index(ty: Type) -> Option<usize> {
    match ty {
        Type::GenericInt => Some(0),
        Type::GenericFloat => Some(1),
        _ => None,
    }
}
fn family(ty: Type, slot: usize) -> bool {
    matches!((ty, slot), (Type::Integer(_), 0) | (Type::Float(_), 1))
}
pub(super) fn active(signature: &Signature) -> [bool; 2] {
    let mut result = [false; 2];
    for ty in signature.parameters.iter().chain([&signature.result]) {
        if let Some(i) = index(*ty) {
            result[i] = true;
        }
    }
    result
}
fn ungroup(mut expr: &Expression) -> &Expression {
    while let ExpressionKind::Group(inner) = &expr.kind {
        expr = inner;
    }
    expr
}
fn candidates(
    signature: &Signature,
    arguments: &[Expression],
    slot: usize,
    scope: &Scope,
    into: &mut Vec<Type>,
) {
    for (parameter, argument) in signature.parameters.iter().zip(arguments) {
        if index(*parameter) != Some(slot) {
            continue;
        }
        let argument = ungroup(argument);
        if let ExpressionKind::Call { name, arguments } = &argument.kind
            && let Some(inner) = scope.functions.get(name)
            && index(inner.result) == Some(slot)
        {
            candidates(inner, arguments, slot, scope, into);
            continue;
        }
        if matches!(
            argument.kind,
            ExpressionKind::Integer { .. } | ExpressionKind::Float(_)
        ) {
            continue;
        }
        let known = expression::operand_type(argument, scope).or_else(|| {
            super::exact::is_operator(argument).then_some(
                if expression::has_float_literal(argument, scope) {
                    Type::Float(FloatType::F64)
                } else {
                    Type::Integer(IntegerType::I64)
                },
            )
        });
        if let Some(ty) = known
            && family(ty, slot)
        {
            into.push(ty);
        }
    }
}
pub(super) fn resolve(
    signature: &Signature,
    arguments: &[Expression],
    expected: Option<Type>,
    scope: &Scope,
    location: Location,
) -> Result<[Option<Type>; 2], Diagnostic> {
    let mut result = [None, None];
    for (slot, used) in active(signature).into_iter().enumerate() {
        if !used {
            continue;
        }
        let mut types = Vec::new();
        candidates(signature, arguments, slot, scope, &mut types);
        if index(signature.result) == Some(slot)
            && let Some(ty) = expected
            && family(ty, slot)
        {
            types.push(ty);
        }
        let mut joined = None;
        for ty in types {
            joined = Some(match joined {
                None => ty,
                Some(from) => super::exact::join(from, ty).ok_or_else(|| {
                    location.error(
                        ErrorKind::InvalidSource,
                        "generic arguments have no common numeric type [num.fn-resolve]",
                    )
                })?,
            });
        }
        result[slot] = Some(joined.unwrap_or(if slot == 0 {
            Type::Integer(IntegerType::I64)
        } else {
            Type::Float(FloatType::F64)
        }));
    }
    Ok(result)
}
pub(super) fn hint(name: &str, arguments: &[Expression], scope: &Scope) -> Option<Type> {
    let signature = scope.functions.get(name)?;
    let Some(slot) = index(signature.result) else {
        return Some(signature.result);
    };
    let mut types = Vec::new();
    candidates(signature, arguments, slot, scope, &mut types);
    expression::joined_type(types.into_iter().map(Some))
}

// Retain the source of inferred local types so policy checks can recompute
// them under each substitution, including the two bounds of a range.
#[derive(Clone)]
pub(super) enum BindingSource {
    Initializer {
        annotation: Option<Type>,
        value: Expression,
    },
    Range {
        start: Expression,
        end: Expression,
    },
}

pub(super) fn missing_handler(location: Location, scope: &Scope, constant: bool) -> Diagnostic {
    if scope.probing_generic {
        scope.missing_cast_handler.set(Some(location));
    }
    location.error(
        ErrorKind::InvalidSource,
        if constant {
            "constant cast is out of range and requires or [const.cast-check]"
        } else {
            "this cast requires or [expr.cast-handler]"
        },
    )
}
