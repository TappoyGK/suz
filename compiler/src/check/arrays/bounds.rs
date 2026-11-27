//! 仕様で定めた条件の覚え方・忘れ方・組み合わせ方だけを範囲保証に使う。
//! 長さの由来は値の一致とは区別し、判定済みの条件は差分制約系で管理する。

mod conditions;
mod writes;

pub(in crate::check) use conditions::{
    block_exits, eval_condition, is_facts_pure, record_binding_remainder, record_checked_access,
    record_match_arm, with_fallback,
};
pub(in crate::check) use writes::{forget_call, forget_loop_writes, forget_mutable_receiver};

use super::{
    super::{
        exact::{Exact, ExactKind},
        ir::{CheckedExpression, CheckedHandler, IntegerOperation, Value},
        scope::Scope,
    },
    facts::{Affine, Facts, Term},
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{BinaryOperator, ComparisonOperator, Location, Type},
};
use std::{cell::RefCell, collections::BTreeMap};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::check) enum Dimension {
    Constant(i128),
    // 定数式は利用箇所へ複製されるので、元の寸法をソース位置で区別する。
    Static(Location, usize, i128),
    Source(usize, usize, Option<i128>),
}
impl Dimension {
    fn known(self) -> Option<i128> {
        match self {
            Self::Constant(n) | Self::Static(_, _, n) => Some(n),
            Self::Source(_, _, n) => n,
        }
    }
}

#[derive(Clone, Default)]
pub(in crate::check) struct State {
    pub arrays: BTreeMap<usize, Vec<Option<Dimension>>>,
    pub names: BTreeMap<usize, Dimension>,
    pub loops: BTreeMap<usize, Loop>,
    pub facts: RefCell<Facts>,
    pub loop_breaks: RefCell<Vec<Option<Vec<Facts>>>>,
}

#[derive(Clone)]
pub(in crate::check) struct Loop {
    length: Dimension,
    start: i128,
    margin: i128,
    location: Location,
}

/// 範囲保証の根拠となった条件のソース位置群。
#[derive(Clone, Default)]
pub(super) struct Proof {
    pub origins: Vec<Location>,
}

impl Proof {
    pub fn merge(&mut self, other: Proof) {
        self.origins.extend(other.origins);
        self.origins.sort_by_key(|l| (l.line, l.column));
        self.origins.dedup();
    }

    pub fn related_diagnostics(&self) -> Vec<Diagnostic> {
        self.origins
            .iter()
            .map(|location| {
                location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("array.bounds-proof", "proven by condition checked here"),
                )
            })
            .collect()
    }
}

fn constant(v: &CheckedExpression) -> Option<i128> {
    super::super::constants::constant_integer(v).ok().flatten()
}

fn field_value(v: &CheckedExpression) -> Option<&CheckedExpression> {
    let Value::Field { target, index } = &v.value else {
        return None;
    };
    let inner = if matches!(target.value, Value::Field { .. }) {
        field_value(target)?
    } else {
        target.as_ref()
    };
    let Value::Struct { fields } = &inner.value else {
        return None;
    };
    fields
        .iter()
        .find(|(id, _)| id == index)
        .map(|(_, value)| value)
}

pub(in crate::check) fn shape(v: &CheckedExpression, scope: &Scope) -> Vec<Option<Dimension>> {
    let Some(id) = v.ty.array_id() else {
        return Vec::new();
    };
    let unknown = || vec![None; scope.structs.arrays.borrow().get(id).rank];
    match &v.value {
        Value::Array {
            metadata, lengths, ..
        } => lengths
            .iter()
            .enumerate()
            .map(|(axis, v)| {
                constant(v).map(|n| {
                    Dimension::Static(metadata.origins[axis].0, metadata.origins[axis].1, n)
                })
            })
            .collect(),
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
        // 構造体の値のフィールドに書いた定数配列の長さは、構造体の全体を評価し直さずに読む。
        Value::Field { .. }
            if field_value(v).is_some_and(|f| matches!(f.value, Value::Array { .. })) =>
        {
            shape(field_value(v).unwrap(), scope)
        }
        _ => {
            if let Ok(Some(evaluated)) = super::super::constants::evaluate(v)
                && let Value::Array {
                    metadata, lengths, ..
                } = &evaluated.value
            {
                return lengths
                    .iter()
                    .enumerate()
                    .map(|(axis, v)| {
                        constant(v).map(|n| {
                            Dimension::Static(metadata.origins[axis].0, metadata.origins[axis].1, n)
                        })
                    })
                    .collect();
            }
            unknown()
        }
    }
}

fn is_nonneg_int(ty: Type) -> bool {
    matches!(
        ty,
        Type::Integer(
            crate::ast::IntegerType::U8
                | crate::ast::IntegerType::U16
                | crate::ast::IntegerType::U32
                | crate::ast::IntegerType::U64
                | crate::ast::IntegerType::Len
        )
    )
}

pub(in crate::check) fn register_nonneg(id: usize, ty: Type, scope: &Scope) {
    if let Type::Range(r) = ty {
        let (lo, hi) = r.bounds();
        scope.array_bounds.facts.borrow_mut().mark_range(id, lo, hi);
    } else if is_nonneg_int(ty) {
        scope.array_bounds.facts.borrow_mut().mark_nonneg(id);
    }
}

pub(in crate::check) fn register(
    id: usize,
    ty: Type,
    mutable: bool,
    mut dimensions: Vec<Option<Dimension>>,
    scope: &mut Scope,
) {
    register_nonneg(id, ty, scope);
    let Some(array) = ty.array_id() else { return };
    if mutable {
        scope.array_bounds.arrays.remove(&id);
        return;
    }
    dimensions.resize(scope.structs.arrays.borrow().get(array).rank, None);
    for (axis, dimension) in dimensions.iter_mut().enumerate() {
        *dimension = Some(match *dimension {
            Some(Dimension::Source(..) | Dimension::Static(..)) => dimension.unwrap(),
            value => Dimension::Source(id, axis, value.and_then(Dimension::known)),
        });
    }
    scope.array_bounds.arrays.insert(id, dimensions);
}

fn length(v: &CheckedExpression, scope: &Scope) -> Option<Dimension> {
    match &v.value {
        Value::Local(id) => scope.array_bounds.names.get(id).copied(),
        Value::Len(target) => target_dimension(target, 0, scope),
        Value::ArrayLength { target, dimension } => target_dimension(target, *dimension, scope),
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

/// `i + c`、`i - c` だけを分離する。入れ子の算術や `or` 付きの算術は対象にしない。
fn offset(v: &CheckedExpression) -> Option<(&CheckedExpression, i128)> {
    match &v.value {
        Value::IntegerArithmetic {
            operation: IntegerOperation::Exact(e),
            handler: None,
            ..
        }
        | Value::Exact(e) => match &e.kind {
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

fn dim_to_affine(dim: Dimension) -> Affine {
    match dim {
        Dimension::Constant(n) | Dimension::Static(_, _, n) => Affine::constant(n),
        Dimension::Source(id, axis, _) => Affine::term(Term::Dim(id, axis)),
    }
}

/// 配列参照 `target` が値経路（ローカル束縛または構造体値フィールド連鎖）であれば `(root_id, fields)` を返す。
/// ポインタの参照解除や非定数配列の要素読み出しを経由する場合は `None` を返す。
fn array_value_path(target: &CheckedExpression) -> Option<(usize, Vec<usize>)> {
    let mut fields = Vec::new();
    let mut cur = target;
    loop {
        match &cur.value {
            Value::Cast(inner) => cur = inner,
            Value::Local(id) => {
                fields.reverse();
                return Some((*id, fields));
            }
            Value::Field { target, index } if matches!(target.ty, Type::Struct(_)) => {
                fields.push(*index);
                cur = target;
            }
            _ => return None,
        }
    }
}

/// ローカル配列束縛の `axis` 番目の寸法を `Vec` を複製せずに引く。
fn target_dimension(target: &CheckedExpression, axis: usize, scope: &Scope) -> Option<Dimension> {
    match &target.value {
        Value::Local(id) => scope
            .array_bounds
            .arrays
            .get(id)
            .and_then(|dims| dims.get(axis).copied().flatten()),
        Value::Cast(inner) => target_dimension(inner, axis, scope),
        _ => shape(target, scope).get(axis).copied().flatten(),
    }
}

/// 配列参照 `target` の `axis` 番目の次元の長さを、仕様 1.1 の項として抽出する。
pub(super) fn target_dim_affine(
    target: &CheckedExpression,
    axis: usize,
    scope: &Scope,
) -> Option<Affine> {
    if let Some(dim) = target_dimension(target, axis, scope) {
        return Some(dim_to_affine(dim));
    }
    let (root, fields) = array_value_path(target)?;
    Some(Affine::term(Term::PathDim(root, fields, axis)))
}

/// 切り出し `target#[start:end]` の長さを `Affine` として抽出する。
pub(super) fn slice_length_affine(
    target: &CheckedExpression,
    start: Option<&CheckedExpression>,
    end: Option<&CheckedExpression>,
    scope: &Scope,
) -> Option<Affine> {
    let start_const = match start {
        None => Some(0),
        Some(expr) => constant(expr),
    };
    let end_const = match end {
        None => target_dimension(target, 0, scope).and_then(Dimension::known),
        Some(expr) => constant(expr),
    };
    if let (Some(a), Some(b)) = (start_const, end_const)
        && 0 <= a
        && a <= b
    {
        return Some(Affine::constant(b - a));
    }
    let s = start_const?;
    let end_affine = match end {
        None => target_dim_affine(target, 0, scope)?,
        Some(expr) => affine(expr, scope)?,
    };
    end_affine.shifted(s.checked_neg()?)
}

/// 書き込み先の対象長さ `out_len` と入力配列 `input` の長さが等しいことを静的に証明できるか判定する。
pub(super) fn proves_same_length(
    out_len: &Affine,
    input: &CheckedExpression,
    scope: &Scope,
) -> bool {
    let Some(input_len) = target_dim_affine(input, 0, scope) else {
        return false;
    };
    if out_len == &input_len {
        return true;
    }
    let facts = scope.array_bounds.facts.borrow();
    facts.proves_le(out_len, &input_len).is_some() && facts.proves_le(&input_len, out_len).is_some()
}

/// 仕様 1.1 の項（整数束縛・`len a`・寸法名・定数式、およびそれぞれへの定数加減算）を抽出する。
pub(in crate::check) fn affine(expr: &CheckedExpression, scope: &Scope) -> Option<Affine> {
    if let Some(n) = constant(expr) {
        return Some(Affine::constant(n));
    }
    let (base, delta) = offset(expr)?;
    let base_affine = base_affine(base, scope)?;
    base_affine.shifted(delta)
}

fn base_affine(expr: &CheckedExpression, scope: &Scope) -> Option<Affine> {
    if let Some(n) = constant(expr) {
        return Some(Affine::constant(n));
    }
    match &expr.value {
        Value::Local(id) => {
            if let Some(&dim) = scope.array_bounds.names.get(id) {
                Some(dim_to_affine(dim))
            } else if expr.ty.is_integer() {
                Some(Affine::term(Term::Local(*id)))
            } else {
                None
            }
        }
        Value::Len(target) => target_dim_affine(target, 0, scope),
        Value::ArrayLength { target, dimension } => target_dim_affine(target, *dimension, scope),
        Value::Cast(inner) if expr.ty.is_integer() && inner.ty.is_integer() => {
            // 暗黙の拡大変換など、元の整数値を変えないキャストのみ透過する。
            let (Some((f_min, f_max)), Some((t_min, t_max))) =
                (inner.ty.integer_bounds(), expr.ty.integer_bounds())
            else {
                return None;
            };
            if f_min >= t_min && f_max <= t_max {
                affine(inner, scope)
            } else {
                None
            }
        }
        Value::Exact(_) => affine(expr, scope),
        _ => None,
    }
}

pub(in crate::check) fn loop_bound(
    id: usize,
    start: &CheckedExpression,
    end: &CheckedExpression,
    inclusive: bool,
    location: Location,
    scope: &mut Scope,
) {
    let start_const = constant(start);
    let i_term = Affine::term(Term::Local(id));
    if let Some(s) = start_const
        .map(Affine::constant)
        .or_else(|| affine(start, scope))
    {
        scope.array_bounds.facts.borrow_mut().add_comparison(
            &s,
            ComparisonOperator::LessEqual,
            &i_term,
            false,
            location,
        );
    }
    let end_offset = offset(end);
    let end_dim = end_offset.and_then(|(base, _)| length(base, scope));
    let end_affine = if let (Some((_, delta)), Some(dim)) = (end_offset, end_dim) {
        dim_to_affine(dim).shifted(delta)
    } else {
        affine(end, scope)
    };
    if let Some(e) = end_affine {
        let op = if inclusive {
            ComparisonOperator::LessEqual
        } else {
            ComparisonOperator::Less
        };
        scope
            .array_bounds
            .facts
            .borrow_mut()
            .add_comparison(&i_term, op, &e, false, location);
    }
    if inclusive {
        return;
    }
    let Some(s) = start_const.filter(|n| *n >= 0) else {
        return;
    };
    let Some((_, delta)) = end_offset.filter(|(_, n)| *n <= 0) else {
        return;
    };
    let Some(length @ (Dimension::Source(..) | Dimension::Static(..))) = end_dim else {
        return;
    };
    let Some(margin) = delta.checked_neg() else {
        return;
    };
    scope.array_bounds.loops.insert(
        id,
        Loop {
            length,
            start: s,
            margin,
            location,
        },
    );
}

pub(super) fn index(
    target: &CheckedExpression,
    value: &CheckedExpression,
    axis: usize,
    need_origins: bool,
    scope: &Scope,
) -> Option<Proof> {
    let dim_opt = target_dimension(target, axis, scope);
    // 既知の定数長と定数添字の組（new #[0; 3] の a.[0] 等）。
    if let Some(n) = dim_opt.and_then(Dimension::known)
        && let Some(i) = constant(value)
        && 0 <= i
        && i < n
    {
        return Some(Proof::default());
    }
    // 値域制限型の値（仮引数・構造体フィールド・ローカル等）は常に LO <= x <= HI を満たす。
    if let Type::Range(r) = value.ty {
        let (lo, hi) = r.bounds();
        if lo >= 0 {
            if let Some(n) = dim_opt.and_then(Dimension::known)
                && hi < n
            {
                return Some(Proof::default());
            }
            let dim = match dim_opt {
                Some(d) => Some(dim_to_affine(d)),
                None => array_value_path(target)
                    .map(|(root, fields)| Affine::term(Term::PathDim(root, fields, axis))),
            };
            if let Some(dim) = dim
                && let Some(origins) = scope
                    .array_bounds
                    .facts
                    .borrow()
                    .proves_lt(&Affine::constant(hi), &dim)
            {
                return Some(Proof { origins });
            }
        }
    }
    // for ループ変数の直接証明を O(1) で判定する。
    if let Some(dimension) = dim_opt
        && let Some((base, c)) = offset(value)
        && let Value::Local(id) = base.value
        && let Some(bound) = scope
            .array_bounds
            .loops
            .get(&id)
            .filter(|b| b.length == dimension)
        && let Some(low) = bound.start.checked_add(c).filter(|n| *n >= 0)
        && c <= bound.margin
    {
        let max_length = dimension.known().unwrap_or(i32::MAX as i128);
        if let Some(high) = max_length
            .checked_sub(bound.margin)
            .and_then(|n| n.checked_sub(1))
            .and_then(|n| n.checked_add(c))
            && value
                .ty
                .integer_bounds()
                .is_some_and(|(min, max)| low >= min && high <= max)
        {
            return Some(if need_origins {
                Proof {
                    origins: vec![bound.location],
                }
            } else {
                Proof::default()
            });
        }
    }
    let dim = match dim_opt {
        Some(d) => dim_to_affine(d),
        None => {
            let (root, fields) = array_value_path(target)?;
            Affine::term(Term::PathDim(root, fields, axis))
        }
    };
    if let Some((m_dim, origins)) = conditions::remainder_dim_proof(value, scope)
        && m_dim == dim
    {
        return Some(Proof { origins });
    }
    let idx = affine(value, scope)?;
    let facts = scope.array_bounds.facts.borrow();
    let low = facts.proves_le(&Affine::constant(0), &idx)?;
    let high = facts.proves_lt(&idx, &dim)?;
    let mut proof = Proof { origins: low };
    proof.merge(Proof { origins: high });
    Some(proof)
}

pub(super) fn slice(
    target: &CheckedExpression,
    start: Option<&CheckedExpression>,
    end: Option<&CheckedExpression>,
    scope: &Scope,
) -> Option<Proof> {
    if start.is_none() && end.is_none() {
        return Some(Proof::default());
    }
    if let Some(n) = target_dimension(target, 0, scope).and_then(Dimension::known)
        && let Some(a) = start.map_or(Some(0), constant)
        && let Some(b) = end.map_or(Some(n), constant)
        && 0 <= a
        && a <= b
        && b <= n
    {
        return Some(Proof::default());
    }
    let dim = target_dim_affine(target, 0, scope)?;
    let s = match start {
        None => Affine::constant(0),
        Some(expr) => affine(expr, scope)?,
    };
    let e = match end {
        None => dim.clone(),
        Some(expr) => affine(expr, scope)?,
    };
    let facts = scope.array_bounds.facts.borrow();
    let o1 = facts.proves_le(&Affine::constant(0), &s)?;
    let o2 = facts.proves_le(&s, &e)?;
    let o3 = facts.proves_le(&e, &dim)?;
    let mut proof = Proof { origins: o1 };
    proof.merge(Proof { origins: o2 });
    proof.merge(Proof { origins: o3 });
    Some(proof)
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

/// 値キャプチャーは参照・寸法名・条件を引き継ぐ。参照キャプチャーの var を含む条件は引き継がない。
pub(in crate::check) fn captures(
    values: &[crate::check::ir::CheckedCapture],
    offset: usize,
    outer: &Scope,
) -> State {
    let mut state = State::default();
    let mut origins = BTreeMap::new();
    let mut dim_map: BTreeMap<(usize, usize), (usize, usize)> = BTreeMap::new();
    let mut local_map: BTreeMap<usize, usize> = BTreeMap::new();
    let mut rename = |dimension| match dimension {
        Dimension::Constant(n) => Dimension::Constant(n),
        Dimension::Static(..) => dimension,
        Dimension::Source(id, axis, known) => {
            let next = origins.len();
            let number = *origins.entry((id, axis)).or_insert(next);
            let mapped = (usize::MAX - number, 0);
            dim_map.insert((id, axis), mapped);
            Dimension::Source(mapped.0, mapped.1, known)
        }
    };
    for (index, capture) in values.iter().enumerate() {
        if capture.by_reference {
            continue;
        }
        let id = offset + index;
        if let Value::Local(old) = capture.value.value {
            local_map.insert(old, id);
        }
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
                    location: bound.location,
                },
            );
        }
        if let Some(dimension) = length(&capture.value, outer) {
            state.names.insert(id, rename(dimension));
        }
    }
    let projected = outer
        .array_bounds
        .facts
        .borrow()
        .project_captures(|term| match term {
            Term::Zero => Some(Term::Zero),
            Term::Local(old) => local_map.get(old).map(|&new_id| Term::Local(new_id)),
            Term::Dim(id, axis) => dim_map
                .get(&(*id, *axis))
                .map(|&(new_id, new_axis)| Term::Dim(new_id, new_axis)),
            Term::PathDim(old_root, fields, axis) => local_map
                .get(old_root)
                .map(|&new_root| Term::PathDim(new_root, fields.clone(), *axis)),
        });
    state.facts = RefCell::new(projected);
    state
}

/// 静的な長さと定数添字の組だけを拒否判定に使う。実行時の値域は推測しない。
pub(super) fn invalid_constant_index(
    target: &CheckedExpression,
    index: &CheckedExpression,
    dimension: usize,
    scope: &Scope,
) -> bool {
    constant(index)
        .zip(target_dimension(target, dimension, scope).and_then(Dimension::known))
        .is_some_and(|(index, length)| !(0..length).contains(&index))
}
