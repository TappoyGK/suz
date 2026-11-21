//! 仕様で定めた形だけを範囲保証に使う。長さの由来は値の一致とは区別する。
use super::super::{
    exact::{Exact, ExactKind},
    ir::{CheckedExpression, CheckedHandler, IntegerOperation, Value},
    scope::Scope,
};
use crate::ast::{BinaryOperator, Type};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::check) enum Dimension {
    Constant(i128),
    Source(usize, usize, Option<i128>),
}
impl Dimension {
    fn known(self) -> Option<i128> {
        match self {
            Self::Constant(n) => Some(n),
            Self::Source(_, _, n) => n,
        }
    }
}
#[derive(Clone, Default)]
pub(in crate::check) struct State {
    pub arrays: BTreeMap<usize, Vec<Option<Dimension>>>,
    pub names: BTreeMap<usize, Dimension>,
    pub loops: BTreeMap<usize, Loop>,
}
#[derive(Clone)]
pub(in crate::check) struct Loop {
    length: Dimension,
    start: i128,
    margin: i128,
}

fn constant(v: &CheckedExpression) -> Option<i128> {
    super::super::constants::constant_integer(v).ok().flatten()
}

pub(in crate::check) fn shape(v: &CheckedExpression, scope: &Scope) -> Vec<Option<Dimension>> {
    let Some(id) = v.ty.array_id() else {
        return Vec::new();
    };
    let unknown = || vec![None; scope.structs.arrays.borrow().get(id).rank];
    match &v.value {
        Value::Local(id) => scope
            .array_bounds
            .arrays
            .get(id)
            .cloned()
            .unwrap_or_else(unknown),
        Value::Cast(inner) => shape(inner, scope),
        Value::New { value, .. } => {
            let Value::Array { lengths, .. } = &value.value else {
                return unknown();
            };
            lengths
                .iter()
                .map(|v| constant(v).map(Dimension::Constant))
                .collect()
        }
        Value::ArrayView {
            target,
            start,
            end,
            index,
            handler,
            ..
        } => {
            let mut result = shape(target, scope);
            if *index {
                result.remove(0);
            } else if start.is_some() || end.is_some() {
                let first = start.as_ref().map_or(Some(0), |v| constant(v));
                let last = end
                    .as_ref()
                    .map_or_else(|| result[0].and_then(Dimension::known), |v| constant(v));
                result[0] = first
                    .zip(last)
                    .filter(|(a, b)| 0 <= *a && a <= b)
                    .map(|(a, b)| Dimension::Constant(b - a));
            }
            if let Some(CheckedHandler::Value(fallback)) = handler
                && !matches!(fallback.value, Value::Panic { .. })
            {
                for (a, b) in result.iter_mut().zip(shape(fallback, scope)) {
                    if *a != b {
                        *a = None;
                    }
                }
            }
            result
        }
        _ => unknown(),
    }
}

pub(in crate::check) fn register(
    id: usize,
    ty: Type,
    mutable: bool,
    mut dimensions: Vec<Option<Dimension>>,
    scope: &mut Scope,
) {
    let Some(array) = ty.array_id() else { return };
    if mutable {
        scope.array_bounds.arrays.remove(&id);
        return;
    }
    dimensions.resize(scope.structs.arrays.borrow().get(array).rank, None);
    for (axis, dimension) in dimensions.iter_mut().enumerate() {
        *dimension = Some(match *dimension {
            Some(Dimension::Source(..)) => dimension.unwrap(),
            value => Dimension::Source(id, axis, value.and_then(Dimension::known)),
        });
    }
    scope.array_bounds.arrays.insert(id, dimensions);
}

fn length(v: &CheckedExpression, scope: &Scope) -> Option<Dimension> {
    match &v.value {
        Value::Local(id) => scope.array_bounds.names.get(id).copied(),
        Value::Len(target) => shape(target, scope).first().copied().flatten(),
        Value::ArrayLength { target, dimension } => {
            shape(target, scope).get(*dimension).copied().flatten()
        }
        _ => None,
    }
}

fn exact_constant(v: &Exact) -> Option<i128> {
    super::super::exact::constant(&CheckedExpression {
        ty: Type::Integer(crate::ast::IntegerType::I64),
        value: Value::Exact(Box::new(v.clone())),
    })?
    .to_i128()
}

/// i+c、i-cだけを分離する。入れ子の算術の代数変形は行わない。
fn offset(v: &CheckedExpression) -> Option<(&CheckedExpression, i128)> {
    match &v.value {
        Value::IntegerArithmetic {
            operation: IntegerOperation::Exact(e),
            handler: None,
            ..
        } => match &e.kind {
            ExactKind::Leaf(v) => Some((v, 0)),
            ExactKind::Binary(op @ (BinaryOperator::Add | BinaryOperator::Subtract), a, b) => {
                let ExactKind::Leaf(a) = &a.kind else {
                    return None;
                };
                let n = exact_constant(b)?;
                Some((
                    a,
                    if matches!(op, BinaryOperator::Subtract) {
                        n.checked_neg()?
                    } else {
                        n
                    },
                ))
            }
            _ => None,
        },
        _ => Some((v, 0)),
    }
}

pub(in crate::check) fn loop_bound(
    id: usize,
    start: &CheckedExpression,
    end: &CheckedExpression,
    inclusive: bool,
    scope: &mut Scope,
) {
    if inclusive {
        return;
    }
    let Some(s) = constant(start).filter(|n| *n >= 0) else {
        return;
    };
    let Some((end, offset)) = offset(end).filter(|(_, n)| *n <= 0) else {
        return;
    };
    let Some(length @ Dimension::Source(..)) = length(end, scope) else {
        return;
    };
    let Some(margin) = offset.checked_neg() else {
        return;
    };
    scope.array_bounds.loops.insert(
        id,
        Loop {
            length,
            start: s,
            margin,
        },
    );
}

pub(super) fn index(
    target: &CheckedExpression,
    value: &CheckedExpression,
    axis: usize,
    scope: &Scope,
) -> bool {
    let Some(dimension) = shape(target, scope).get(axis).copied().flatten() else {
        return false;
    };
    if let (Some(n), Some(i)) = (dimension.known(), constant(value)) {
        return 0 <= i && i < n;
    }
    let Some((base, c)) = offset(value) else {
        return false;
    };
    let Value::Local(id) = base.value else {
        return false;
    };
    let Some(bound) = scope
        .array_bounds
        .loops
        .get(&id)
        .filter(|b| b.length == dimension)
    else {
        return false;
    };
    let Some(low) = bound.start.checked_add(c).filter(|n| *n >= 0) else {
        return false;
    };
    if c > bound.margin {
        return false;
    }
    let max_length = dimension.known().unwrap_or(i32::MAX as i128);
    let Some(high) = max_length
        .checked_sub(bound.margin)
        .and_then(|n| n.checked_sub(1))
        .and_then(|n| n.checked_add(c))
    else {
        return false;
    };
    value
        .ty
        .integer_bounds()
        .is_some_and(|(min, max)| low >= min && high <= max)
}

pub(super) fn slice(
    target: &CheckedExpression,
    start: Option<&CheckedExpression>,
    end: Option<&CheckedExpression>,
    scope: &Scope,
) -> bool {
    if start.is_none() && end.is_none() {
        return true;
    }
    let Some(n) = shape(target, scope)
        .first()
        .copied()
        .flatten()
        .and_then(Dimension::known)
    else {
        return false;
    };
    let Some(a) = start.map_or(Some(0), constant) else {
        return false;
    };
    let Some(b) = end.map_or(Some(n), constant) else {
        return false;
    };
    0 <= a && a <= b && b <= n
}

/// 保証済みの添字に限り、C生成用の中間値域を反復区間まで狭める。
/// ソースの算術式の受理・拒否を変える一般的な範囲推論には使わない。
pub(super) fn narrow(value: &mut CheckedExpression, scope: &Scope) {
    let Some((base, c)) = offset(value) else {
        return;
    };
    let Value::Local(id) = base.value else { return };
    let Some(bound) = scope.array_bounds.loops.get(&id) else {
        return;
    };
    let low = bound.start;
    let high = bound.length.known().unwrap_or(i32::MAX as i128) - bound.margin - 1;
    if low > high {
        return;
    }
    let Value::IntegerArithmetic {
        operation: IntegerOperation::Exact(e),
        ..
    } = &mut value.value
    else {
        return;
    };
    e.range = Some(crate::numeric::Range::bounds(low + c, high + c));
    if let ExactKind::Binary(_, left, _) = &mut e.kind {
        left.range = Some(crate::numeric::Range::bounds(low, high));
    }
}

/// 値キャプチャーは参照と寸法名の関係を保つ。外側のローカル番号を本体の番号と混同しない。
pub(in crate::check) fn captures(
    values: &[crate::check::ir::CheckedCapture],
    offset: usize,
    outer: &Scope,
) -> State {
    let mut state = State::default();
    let mut origins = BTreeMap::new();
    let mut rename = |dimension| match dimension {
        Dimension::Constant(n) => Dimension::Constant(n),
        Dimension::Source(id, axis, known) => {
            let next = origins.len();
            let number = *origins.entry((id, axis)).or_insert(next);
            Dimension::Source(usize::MAX - number, 0, known)
        }
    };
    for (index, capture) in values.iter().enumerate() {
        if capture.by_reference {
            continue;
        }
        let id = offset + index;
        if capture.value.ty.array_id().is_some() {
            state.arrays.insert(
                id,
                shape(&capture.value, outer)
                    .into_iter()
                    .map(|d| d.map(&mut rename))
                    .collect(),
            );
        }
        if let Value::Local(old) = capture.value.value
            && let Some(bound) = outer.array_bounds.loops.get(&old)
        {
            state.loops.insert(
                id,
                Loop {
                    length: rename(bound.length),
                    start: bound.start,
                    margin: bound.margin,
                },
            );
        }
        if let Some(dimension) = length(&capture.value, outer) {
            state.names.insert(id, rename(dimension));
        }
    }
    state
}
