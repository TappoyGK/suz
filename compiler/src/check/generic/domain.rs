//! Numeric type functions used by the single generic-body traversal.
//! Indices identify the same int/float substitution across all operands. We
//! enumerate only a rule's scalar inputs, never replay statements or expressions.
use crate::ast::{FloatType, IntegerType, ScalarType, Type};

pub(super) const INTS: [IntegerType; 8] = [
    IntegerType::I8,
    IntegerType::I16,
    IntegerType::I32,
    IntegerType::I64,
    IntegerType::U8,
    IntegerType::U16,
    IntegerType::U32,
    IntegerType::U64,
];

pub(crate) fn at(ty: Type, index: usize) -> Option<Type> {
    match ty {
        Type::GenericInt => Some(Type::Integer(INTS[index / 2])),
        Type::GenericFloat => Some(Type::Float(if index.is_multiple_of(2) {
            FloatType::F32
        } else {
            FloatType::F64
        })),
        Type::Numeric(types) => types[index].map(|ty| match ty {
            ScalarType::Integer(ty) => Type::Integer(ty),
            ScalarType::Float(ty) => Type::Float(ty),
        }),
        _ => Some(ty),
    }
}

pub(crate) fn from_types(types: [Option<Type>; 16]) -> Option<Type> {
    if types.iter().all(Option::is_none) {
        return None;
    }
    if types.iter().all(|ty| *ty == types[0]) {
        return types[0];
    }
    for ty in [Type::GenericInt, Type::GenericFloat] {
        if (0..16).all(|i| types[i] == at(ty, i)) {
            return Some(ty);
        }
    }
    Some(Type::Numeric(types.map(|ty| {
        ty.map(|ty| match ty {
            Type::Integer(ty) => ScalarType::Integer(ty),
            Type::Float(ty) => ScalarType::Float(ty),
            _ => unreachable!("numeric rules produce scalar numeric types"),
        })
    })))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Judgment {
    Valid,
    Invalid,
    Deferred,
}
impl Judgment {
    pub fn possible(self) -> bool {
        self != Self::Invalid
    }
}

// [num.fn-check-judge]: all valid, all invalid, or deferred until instantiation.
pub(crate) fn judge(a: Type, b: Type, mut predicate: impl FnMut(Type, Type) -> bool) -> Judgment {
    if !a.symbolic() && !b.symbolic() {
        return if predicate(a, b) {
            Judgment::Valid
        } else {
            Judgment::Invalid
        };
    }
    let (mut yes, mut no) = (false, false);
    for i in 0..16 {
        if let (Some(a), Some(b)) = (at(a, i), at(b, i)) {
            if predicate(a, b) {
                yes = true;
            } else {
                no = true;
            }
        }
    }
    match (yes, no) {
        (true, false) => Judgment::Valid,
        (true, true) => Judgment::Deferred,
        _ => Judgment::Invalid,
    }
}

pub(crate) fn same(a: Type, b: Type) -> bool {
    judge(a, b, |a, b| a == b).possible()
}

pub(crate) fn bounds(ty: Type) -> Option<(i128, i128)> {
    if !ty.symbolic() {
        return ty.integer_bounds();
    }
    (0..16)
        .filter_map(|i| at(ty, i)?.integer_bounds())
        .reduce(|(lo, hi), (a, b)| (lo.min(a), hi.max(b)))
}
