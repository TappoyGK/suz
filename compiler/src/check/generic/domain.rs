//! intとfloatの型の組み合わせごとに、型・値域・規則の成否を求める。全オペランドで同じ添字が同じ型の代入を表す。
//!
//! 利用元: [`crate::check::expression`]、[`crate::check::exact`]。

use crate::ast::{FloatType, IntegerType, ScalarType, Type};

pub(crate) use crate::ast::NUMERIC_COMBINATIONS;
pub(crate) fn indices() -> std::ops::Range<usize> {
    0..NUMERIC_COMBINATIONS
}
pub(crate) fn any(predicate: impl FnMut(usize) -> bool) -> bool {
    indices().any(predicate)
}
pub(crate) fn uniform<T: PartialEq>(mut value: impl FnMut(usize) -> Option<T>) -> Option<T> {
    let first = value(0)?;
    for index in indices().skip(1) {
        if value(index)? != first {
            return None;
        }
    }
    Some(first)
}
pub(crate) fn try_all<E>(mut predicate: impl FnMut(usize) -> Result<bool, E>) -> Result<bool, E> {
    for index in indices() {
        if !predicate(index)? {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(crate) fn judge_one(ty: Type, mut predicate: impl FnMut(Type) -> bool) -> Judgment {
    judge(ty, ty, |ty, _| predicate(ty))
}

pub(super) const INTS: [IntegerType; 9] = [
    IntegerType::I8,
    IntegerType::I16,
    IntegerType::I32,
    IntegerType::I64,
    IntegerType::U8,
    IntegerType::U16,
    IntegerType::U32,
    IntegerType::U64,
    IntegerType::Len,
];

pub(crate) fn at(ty: Type, index: usize) -> Option<Type> {
    match ty {
        Type::Pointer { .. } => at(ty.pointee().unwrap(), index).map(|inner| {
            inner
                .pointer()
                .with_readonly(ty.is_readonly_pointer())
                .with_nullable(ty.is_nullable())
        }),
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

pub(crate) fn from_types(types: [Option<Type>; NUMERIC_COMBINATIONS]) -> Option<Type> {
    if types.iter().all(Option::is_none) {
        return None;
    }
    if types.iter().all(|ty| *ty == types[0]) {
        return types[0];
    }
    for ty in [Type::GenericInt, Type::GenericFloat] {
        if indices().all(|i| types[i] == at(ty, i)) {
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

// [num.fn-check-judge]: すべて成功、すべて失敗、または具体化まで保留のいずれかに分類する。
pub(crate) fn judge(a: Type, b: Type, predicate: impl FnMut(Type, Type) -> bool) -> Judgment {
    judge_with_failures(a, b, predicate).0
}

pub(crate) fn judge_with_failures(
    a: Type,
    b: Type,
    mut predicate: impl FnMut(Type, Type) -> bool,
) -> (Judgment, u32) {
    if !a.leaf().symbolic() && !b.leaf().symbolic() {
        return if predicate(a, b) {
            (Judgment::Valid, 0)
        } else {
            (Judgment::Invalid, u32::MAX)
        };
    }
    let mut yes = false;
    let mut failures = 0;
    for i in indices() {
        if let (Some(a), Some(b)) = (at(a, i), at(b, i)) {
            if predicate(a, b) {
                yes = true;
            } else {
                failures |= 1 << i;
            }
        }
    }
    (
        match (yes, failures != 0) {
            (true, false) => Judgment::Valid,
            (true, true) => Judgment::Deferred,
            _ => Judgment::Invalid,
        },
        failures,
    )
}

pub(crate) fn same(a: Type, b: Type) -> bool {
    a == b || judge(a, b, |a, b| a == b).possible()
}

pub(crate) fn bounds(ty: Type) -> Option<(i128, i128)> {
    if !ty.symbolic() {
        return ty.integer_bounds();
    }
    indices()
        .filter_map(|i| at(ty, i)?.integer_bounds())
        .reduce(|(lo, hi), (a, b)| (lo.min(a), hi.max(b)))
}
