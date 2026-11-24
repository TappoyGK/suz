//! 仕様で定めた条件の覚え方・忘れ方・組み合わせ方だけを範囲保証に使う。
//! 長さの由来は値の一致とは区別し、判定済みの条件は差分制約系で管理する。
use super::{
    super::{
        exact::{Exact, ExactKind},
        ir::{CheckedExpression, CheckedHandler, CheckedStatement, IntegerOperation, Value},
        scope::Scope,
    },
    facts::{Affine, Facts, Origins, Term},
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{
        BinaryOperator, ComparisonOperator, Expression, ExpressionKind, Location, LogicalOperator,
        Statement, Type,
    },
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
    if is_nonneg_int(ty) {
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
        Value::Exact(e) => match &e.kind {
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
fn target_dim_affine(target: &CheckedExpression, axis: usize, scope: &Scope) -> Option<Affine> {
    if let Some(dim) = target_dimension(target, axis, scope) {
        return Some(dim_to_affine(dim));
    }
    let (root, fields) = array_value_path(target)?;
    Some(Affine::term(Term::PathDim(root, fields, axis)))
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

pub(in crate::check) fn record_match_arm(
    target: &CheckedExpression,
    pattern: &CheckedExpression,
    location: Location,
    scope: &Scope,
) {
    let Some(t) = affine(target, scope) else {
        return;
    };
    let Some(c) = constant(pattern) else {
        return;
    };
    scope.array_bounds.facts.borrow_mut().add_comparison(
        &t,
        ComparisonOperator::Equal,
        &Affine::constant(c),
        false,
        location,
    );
}

/// `Exact` 式が `0 <= expr` を満たすかを判定し、根拠位置を返す。
fn proves_exact_nonneg(exact: &Exact, scope: &Scope) -> Option<Vec<Location>> {
    if let Some(n) = exact_constant(exact) {
        return (n >= 0).then(Vec::new);
    }
    if let ExactKind::Leaf(expr) = &exact.kind {
        if is_nonneg_int(expr.ty) {
            return Some(Vec::new());
        }
        let a = affine(expr, scope)?;
        return scope
            .array_bounds
            .facts
            .borrow()
            .proves_le(&Affine::constant(0), &a);
    }
    let wrapped = CheckedExpression {
        ty: Type::Integer(crate::ast::IntegerType::I64),
        value: Value::Exact(Box::new(exact.clone())),
    };
    let a = affine(&wrapped, scope)?;
    scope
        .array_bounds
        .facts
        .borrow()
        .proves_le(&Affine::constant(0), &a)
}

/// `Exact` 式が配列の `len a` または寸法名であれば、対応する `Affine` を返す。
fn exact_dim_expr(exact: &Exact, scope: &Scope) -> Option<Affine> {
    let ExactKind::Leaf(expr) = &exact.kind else {
        return None;
    };
    dim_expr_affine(expr, scope)
}

fn dim_expr_affine(expr: &CheckedExpression, scope: &Scope) -> Option<Affine> {
    match &expr.value {
        Value::Local(id) => scope.array_bounds.names.get(id).copied().map(dim_to_affine),
        Value::Len(target) => target_dim_affine(target, 0, scope),
        Value::ArrayLength { target, dimension } => target_dim_affine(target, *dimension, scope),
        Value::Cast(inner) if expr.ty.is_integer() && inner.ty.is_integer() => {
            dim_expr_affine(inner, scope)
        }
        Value::Exact(e) => exact_dim_expr(e, scope),
        _ => None,
    }
}

/// `x % m`（`or` ハンドラなし）で `0 <= x` が導け、かつ `m` が `len a` または寸法名なら `(m, origins)` を返す。
fn remainder_dim_proof(expr: &CheckedExpression, scope: &Scope) -> Option<(Affine, Vec<Location>)> {
    let exact = match &expr.value {
        Value::IntegerArithmetic {
            operation: IntegerOperation::Exact(e),
            handler: None,
            ..
        } => e.as_ref(),
        Value::Exact(e) => e.as_ref(),
        Value::Cast(inner) if expr.ty.is_integer() && inner.ty.is_integer() => {
            return remainder_dim_proof(inner, scope);
        }
        _ => return None,
    };
    let ExactKind::Binary(BinaryOperator::Remainder, left, right) = &exact.kind else {
        return None;
    };
    let dim = exact_dim_expr(right, scope)?;
    let origins = proves_exact_nonneg(left, scope)?;
    Some((dim, origins))
}

/// `fix idx = x % len a` のように剰余を束縛したとき、`0 <= idx < len a` を覚える。
pub(in crate::check) fn record_binding_remainder(
    id: usize,
    initializer: &CheckedExpression,
    location: Location,
    scope: &Scope,
) {
    let Some((dim, mut origins)) = remainder_dim_proof(initializer, scope) else {
        return;
    };
    if !origins.contains(&location) {
        origins.push(location);
    }
    let idx = Affine::term(Term::Local(id));
    let origins = Origins::from_vec(origins);
    let mut facts = scope.array_bounds.facts.borrow_mut();
    facts.add_diff(Term::Zero, Term::Local(id), 0, origins.clone());
    if let Some(bound) = dim.offset.checked_sub(1) {
        facts.add_diff(idx.term, dim.term, bound, origins);
    }
}

/// 式が条件集合を一切変化させない単純なリテラル・変数参照であるかを返す。
pub(in crate::check) fn is_facts_pure(expr: &Expression) -> bool {
    match &expr.kind {
        ExpressionKind::Integer { .. }
        | ExpressionKind::Float(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Char(_)
        | ExpressionKind::Nil
        | ExpressionKind::Void
        | ExpressionKind::Name(_)
        | ExpressionKind::Variant { .. } => true,
        ExpressionKind::Group(inner)
        | ExpressionKind::Negate(inner)
        | ExpressionKind::BitNot(inner) => is_facts_pure(inner),
        _ => false,
    }
}

/// 条件付きで評価される代替値（`or 式`・`or return`・`or error`）を検査する。
/// 脱出するハンドラでは事前状態を復元し、値を返すハンドラでは成功経路と失敗経路を合流する。
pub(in crate::check) fn with_fallback<T>(
    scope: &Scope,
    exits: bool,
    f: impl FnOnce() -> Result<T, Diagnostic>,
) -> Result<T, Diagnostic> {
    let before = scope.array_bounds.facts.borrow().clone();
    let result = f();
    if result.is_ok() {
        if exits {
            *scope.array_bounds.facts.borrow_mut() = before;
        } else {
            let after = scope.array_bounds.facts.borrow().clone();
            *scope.array_bounds.facts.borrow_mut() = Facts::join_all(&[before, after]);
        }
    } else {
        *scope.array_bounds.facts.borrow_mut() = before;
    }
    result
}

/// 範囲外で抜ける（`or return` / `or panic`）配列読み出し・切り出しの後ろで、通過した範囲条件を覚える。
pub(in crate::check) fn record_checked_access(expr: &CheckedExpression, scope: &Scope) {
    match &expr.value {
        Value::ArrayRead { address, .. } => {
            if let Value::ArrayElement { target, indices } = &address.value {
                record_checked_access(target, scope);
                for (axis, (idx_expr, loc, _)) in indices.iter().enumerate() {
                    if let (Some(dim), Some(idx)) = (
                        target_dim_affine(target, axis, scope),
                        affine(idx_expr, scope),
                    ) {
                        let mut facts = scope.array_bounds.facts.borrow_mut();
                        facts.add_comparison(
                            &Affine::constant(0),
                            ComparisonOperator::LessEqual,
                            &idx,
                            false,
                            *loc,
                        );
                        facts.add_comparison(&idx, ComparisonOperator::Less, &dim, false, *loc);
                    }
                }
            }
        }
        Value::ArrayView {
            target,
            start,
            end,
            index,
            location,
            ..
        } => {
            record_checked_access(target, scope);
            if let Some(dim) = target_dim_affine(target, 0, scope) {
                if *index {
                    if let Some(idx_expr) = start.as_deref()
                        && let Some(idx) = affine(idx_expr, scope)
                    {
                        let mut facts = scope.array_bounds.facts.borrow_mut();
                        facts.add_comparison(
                            &Affine::constant(0),
                            ComparisonOperator::LessEqual,
                            &idx,
                            false,
                            *location,
                        );
                        facts.add_comparison(
                            &idx,
                            ComparisonOperator::Less,
                            &dim,
                            false,
                            *location,
                        );
                    }
                } else {
                    let s = match start.as_deref() {
                        None => Some(Affine::constant(0)),
                        Some(e) => affine(e, scope),
                    };
                    let e = match end.as_deref() {
                        None => Some(dim.clone()),
                        Some(ex) => affine(ex, scope),
                    };
                    if let (Some(s), Some(e)) = (s, e) {
                        let mut facts = scope.array_bounds.facts.borrow_mut();
                        facts.add_comparison(
                            &Affine::constant(0),
                            ComparisonOperator::LessEqual,
                            &s,
                            false,
                            *location,
                        );
                        facts.add_comparison(
                            &s,
                            ComparisonOperator::LessEqual,
                            &e,
                            false,
                            *location,
                        );
                        facts.add_comparison(
                            &e,
                            ComparisonOperator::LessEqual,
                            &dim,
                            false,
                            *location,
                        );
                    }
                }
            }
        }
        _ => {}
    }
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
    if let Some((m_dim, origins)) = remainder_dim_proof(value, scope)
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
    if let Some(n) = shape(target, scope)
        .first()
        .copied()
        .flatten()
        .and_then(Dimension::known)
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
        .zip(
            shape(target, scope)
                .get(dimension)
                .copied()
                .flatten()
                .and_then(Dimension::known),
        )
        .is_some_and(|(index, length)| !(0..length).contains(&index))
}

/// 関数呼び出しにより書き換わりうる、参照キャプチャー済みの var の条件を忘れる。
pub(in crate::check) fn forget_call(scope: &Scope) {
    let captured = scope.ref_captured.borrow();
    if captured.is_empty() {
        return;
    }
    let mut facts = scope.array_bounds.facts.borrow_mut();
    for &id in captured.iter() {
        facts.forget_local(id);
    }
}

/// `#` メソッドの受け手が `var` 変数またはそのフィールド経路であれば、その条件を忘れる。
pub(in crate::check) fn forget_mutable_receiver(receiver: &Expression, scope: &Scope) {
    if let Some((id, fields)) = mutable_ast_path(receiver, scope) {
        scope
            .array_bounds
            .facts
            .borrow_mut()
            .forget_path(id, &fields);
    }
}

fn mutable_ast_path(expr: &Expression, scope: &Scope) -> Option<(usize, Vec<usize>)> {
    let mut names = Vec::new();
    let mut cur = expr;
    loop {
        match &cur.kind {
            ExpressionKind::Group(inner) => cur = inner,
            ExpressionKind::Field { target, name } => {
                names.push(name.as_str());
                cur = target;
            }
            ExpressionKind::Name(name) => {
                let binding = scope.locals.get(name)?;
                if !binding.mutable {
                    return None;
                }
                names.reverse();
                let mut ty = binding.ty;
                let mut indices = Vec::new();
                for field_name in names {
                    let Type::Struct(sid) = ty else {
                        return None;
                    };
                    let s = scope.structs.get(sid);
                    let idx = s.fields.iter().position(|f| f.name == field_name)?;
                    ty = s.fields[idx].ty;
                    indices.push(idx);
                }
                return Some((binding.id, indices));
            }
            _ => return None,
        }
    }
}

/// ループ本体（および `while` 条件）で書き換わりうる項を事前走査してループ先頭で忘れる。
pub(in crate::check) fn forget_loop_writes(
    statements: &[Statement],
    extra_expr: Option<&Expression>,
    scope: &Scope,
) {
    let mut facts = scope.array_bounds.facts.borrow_mut();
    if facts.is_empty() {
        return;
    }
    if let Some(expr) = extra_expr {
        forget_expr_writes_into(expr, scope, &mut facts);
    }
    for stmt in statements {
        forget_stmt_writes_into(stmt, scope, &mut facts);
    }
}

fn forget_stmt_writes_into(stmt: &Statement, scope: &Scope, facts: &mut Facts) {
    match stmt {
        Statement::Assign {
            name,
            fields,
            value,
            ..
        } => {
            if let Some(binding) = scope.locals.get(name)
                && binding.mutable
            {
                let mut ty = binding.ty;
                let mut indices = Vec::new();
                let mut ok = true;
                for field_name in fields {
                    if let Type::Struct(sid) = ty
                        && let Some(idx) = scope
                            .structs
                            .get(sid)
                            .fields
                            .iter()
                            .position(|f| f.name == *field_name)
                    {
                        ty = scope.structs.get(sid).fields[idx].ty;
                        indices.push(idx);
                    } else {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    facts.forget_path(binding.id, &indices);
                } else {
                    facts.forget_local(binding.id);
                }
            }
            forget_expr_writes_into(value, scope, facts);
        }
        Statement::Binding { initializer, .. } => {
            forget_expr_writes_into(initializer, scope, facts)
        }
        Statement::Expression(e)
        | Statement::Return(e)
        | Statement::Fail(e)
        | Statement::Recover(e) => forget_expr_writes_into(e, scope, facts),
        Statement::Store { target, value, .. } => {
            forget_expr_writes_into(target, scope, facts);
            forget_expr_writes_into(value, scope, facts);
        }
        Statement::Log { parts, .. } => {
            for part in parts {
                if let crate::ast::TemplatePart::Value(e) = part {
                    forget_expr_writes_into(e, scope, facts);
                }
            }
        }
        Statement::If {
            branches,
            otherwise,
        } => {
            for (cond, body) in branches {
                forget_expr_writes_into(cond, scope, facts);
                for s in body {
                    forget_stmt_writes_into(s, scope, facts);
                }
            }
            if let Some(body) = otherwise {
                for s in body {
                    forget_stmt_writes_into(s, scope, facts);
                }
            }
        }
        Statement::Loop { condition, body } => {
            if let Some(c) = condition {
                forget_expr_writes_into(c, scope, facts);
            }
            for s in body {
                forget_stmt_writes_into(s, scope, facts);
            }
        }
        Statement::For {
            start, end, body, ..
        } => {
            forget_expr_writes_into(start, scope, facts);
            if let Some(e) = end {
                forget_expr_writes_into(e, scope, facts);
            }
            for s in body {
                forget_stmt_writes_into(s, scope, facts);
            }
        }
        Statement::Match { target, arms, .. } => {
            forget_expr_writes_into(target, scope, facts);
            for arm in arms {
                for s in &arm.body {
                    forget_stmt_writes_into(s, scope, facts);
                }
            }
        }
        Statement::Let {
            value,
            body,
            otherwise,
            ..
        } => {
            forget_expr_writes_into(value, scope, facts);
            for s in body {
                forget_stmt_writes_into(s, scope, facts);
            }
            for s in otherwise {
                forget_stmt_writes_into(s, scope, facts);
            }
        }
        Statement::Block(body)
        | Statement::Trusted(body)
        | Statement::Objection(body)
        | Statement::Arena { body, .. } => {
            for s in body {
                forget_stmt_writes_into(s, scope, facts);
            }
        }
        Statement::Break(_) | Statement::Continue(_) | Statement::Ignore(_) => {}
    }
}

fn forget_expr_writes_into(expr: &Expression, scope: &Scope, facts: &mut Facts) {
    match &expr.kind {
        ExpressionKind::Call { arguments, .. } => {
            for arg in arguments {
                forget_expr_writes_into(arg, scope, facts);
            }
            for &id in scope.ref_captured.borrow().iter() {
                facts.forget_local(id);
            }
        }
        ExpressionKind::Invoke {
            target, arguments, ..
        } => {
            forget_expr_writes_into(target, scope, facts);
            for arg in arguments {
                forget_expr_writes_into(arg, scope, facts);
            }
            for &id in scope.ref_captured.borrow().iter() {
                facts.forget_local(id);
            }
        }
        ExpressionKind::MethodCall {
            target,
            mutable,
            arguments,
            ..
        } => {
            forget_expr_writes_into(target, scope, facts);
            for arg in arguments {
                forget_expr_writes_into(arg, scope, facts);
            }
            if *mutable && let Some((id, fields)) = mutable_ast_path(target, scope) {
                facts.forget_path(id, &fields);
            }
            for &id in scope.ref_captured.borrow().iter() {
                facts.forget_local(id);
            }
        }
        ExpressionKind::Closure(closure) => {
            for cap in &closure.captures {
                if cap.by_reference
                    && let Some(binding) = scope.locals.get(&cap.name)
                {
                    facts.forget_local(binding.id);
                }
            }
        }
        ExpressionKind::Block { statements, value } => {
            for s in statements {
                forget_stmt_writes_into(s, scope, facts);
            }
            forget_expr_writes_into(value, scope, facts);
        }
        ExpressionKind::Group(e)
        | ExpressionKind::Negate(e)
        | ExpressionKind::BitNot(e)
        | ExpressionKind::Not(e)
        | ExpressionKind::New(e)
        | ExpressionKind::Deref(e)
        | ExpressionKind::Len(e)
        | ExpressionKind::CBytes(e)
        | ExpressionKind::CByteLen(e)
        | ExpressionKind::Field { target: e, .. } => forget_expr_writes_into(e, scope, facts),
        ExpressionKind::Cast {
            operand, fallback, ..
        }
        | ExpressionKind::ArithmeticHandler { operand, fallback } => {
            forget_expr_writes_into(operand, scope, facts);
            if let Some(fb) = fallback {
                forget_handler_writes_into(fb, scope, facts);
            }
        }
        ExpressionKind::TextAppend { left, right }
        | ExpressionKind::Binary { left, right, .. }
        | ExpressionKind::Logical { left, right, .. } => {
            forget_expr_writes_into(left, scope, facts);
            forget_expr_writes_into(right, scope, facts);
        }
        ExpressionKind::TextBuffer { capacity, value } => {
            if let Some(cap) = capacity {
                forget_expr_writes_into(cap, scope, facts);
            }
            forget_expr_writes_into(value, scope, facts);
        }
        ExpressionKind::Comparison { operands, .. } => {
            for o in operands {
                forget_expr_writes_into(o, scope, facts);
            }
        }
        ExpressionKind::Slice {
            target,
            start,
            end,
            fallback,
            ..
        } => {
            forget_expr_writes_into(target, scope, facts);
            if let Some(s) = start {
                forget_expr_writes_into(s, scope, facts);
            }
            if let Some(e) = end {
                forget_expr_writes_into(e, scope, facts);
            }
            if let Some(fb) = fallback {
                forget_handler_writes_into(fb, scope, facts);
            }
        }
        ExpressionKind::If {
            branches,
            otherwise,
        } => {
            for (c, v) in branches {
                forget_expr_writes_into(c, scope, facts);
                forget_expr_writes_into(v, scope, facts);
            }
            forget_expr_writes_into(otherwise, scope, facts);
        }
        ExpressionKind::Match { target, arms } => {
            forget_expr_writes_into(target, scope, facts);
            for arm in arms {
                forget_expr_writes_into(&arm.body, scope, facts);
            }
        }
        ExpressionKind::Struct { fields, .. } => {
            for f in fields {
                forget_expr_writes_into(&f.value, scope, facts);
            }
        }
        ExpressionKind::With { target, fields } => {
            forget_expr_writes_into(target, scope, facts);
            for f in fields {
                forget_expr_writes_into(&f.value, scope, facts);
            }
        }
        ExpressionKind::ArrayInit {
            values, lengths, ..
        } => {
            for v in values {
                forget_expr_writes_into(v, scope, facts);
            }
            for l in lengths {
                forget_expr_writes_into(l, scope, facts);
            }
        }
        ExpressionKind::Template(parts) | ExpressionKind::Panic(parts) => {
            for part in parts {
                if let crate::ast::TemplatePart::Value(e) = part {
                    forget_expr_writes_into(e, scope, facts);
                }
            }
        }
        _ => {}
    }
}

fn forget_handler_writes_into(
    handler: &crate::ast::FailureHandler,
    scope: &Scope,
    facts: &mut Facts,
) {
    match handler {
        crate::ast::FailureHandler::Value(e)
        | crate::ast::FailureHandler::Return { value: e, .. } => {
            forget_expr_writes_into(e, scope, facts)
        }
        crate::ast::FailureHandler::Fail { value: Some(e), .. } => {
            forget_expr_writes_into(e, scope, facts)
        }
        crate::ast::FailureHandler::Errors(err) => {
            for arm in &err.arms {
                match &arm.body {
                    crate::ast::ErrorArmBody::Value(e) => forget_expr_writes_into(e, scope, facts),
                    crate::ast::ErrorArmBody::Statements { body, .. } => {
                        for s in body {
                            forget_stmt_writes_into(s, scope, facts);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// 論理式・比較式を評価順（短絡評価・比較連鎖を含む）で検査し、
/// 真の側・偽の側で成り立つ `Facts` をそれぞれ返す。
pub(in crate::check) fn eval_condition(
    expr: &Expression,
    scope: &Scope,
) -> Result<(CheckedExpression, Facts, Facts), Diagnostic> {
    match &expr.kind {
        ExpressionKind::Group(inner) => eval_condition(inner, scope),
        ExpressionKind::Not(inner) => {
            // ユーザー定義の not 演算子メソッドの場合は通常の式として扱う。
            if scope.has_operator_methods
                && super::super::operator_methods::unary_name("not", inner, scope)
                    .is_some_and(|name| scope.functions.contains_key(&name))
            {
                let checked = super::super::expression::expression(expr, None, scope)?;
                let after = scope.array_bounds.facts.borrow().clone();
                return Ok((checked, after.clone(), after));
            }
            let (operand, t, f) = eval_condition(inner, scope)?;
            if operand.ty != Type::Bool {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    if matches!(operand.ty, Type::Struct(_) | Type::Enum(_)) {
                        crate::diagnostic!(
                            "operator.unary",
                            "no unary operator not defined for this type"
                        )
                    } else {
                        crate::diagnostic!("expr.not", "not requires a bool operand")
                    },
                ));
            }
            Ok((
                CheckedExpression {
                    ty: Type::Bool,
                    value: Value::Not(Box::new(operand)),
                },
                f,
                t,
            ))
        }
        ExpressionKind::Logical {
            operator,
            left,
            right,
        } => {
            let (checked_left, left_t, left_f) = eval_condition(left, scope)?;
            if checked_left.ty != Type::Bool {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "expr.operator-types",
                        "{} requires bool operands",
                        operator.symbol()
                    ),
                ));
            }
            let saved = scope.array_bounds.facts.borrow().clone();
            let (checked_right, right_t, right_f) = match operator {
                LogicalOperator::And => {
                    *scope.array_bounds.facts.borrow_mut() = left_t.clone();
                    eval_condition(right, scope)?
                }
                LogicalOperator::Or => {
                    *scope.array_bounds.facts.borrow_mut() = left_f.clone();
                    eval_condition(right, scope)?
                }
            };
            *scope.array_bounds.facts.borrow_mut() = saved;
            if checked_right.ty != Type::Bool {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "expr.operator-types",
                        "{} requires bool operands",
                        operator.symbol()
                    ),
                ));
            }
            let (true_facts, false_facts) = match operator {
                LogicalOperator::And => (right_t, Facts::join_all(&[left_f, right_f])),
                LogicalOperator::Or => (Facts::join_all(&[left_t, right_t]), right_f),
            };
            Ok((
                CheckedExpression {
                    ty: Type::Bool,
                    value: Value::Logical {
                        operator: *operator,
                        left: Box::new(checked_left),
                        right: Box::new(checked_right),
                    },
                },
                true_facts,
                false_facts,
            ))
        }
        ExpressionKind::Comparison {
            locations,
            operators,
            operands: ast_operands,
        } => {
            let checked = super::super::expression::expression(expr, None, scope)?;
            let after = scope.array_bounds.facts.borrow().clone();
            let mut true_facts = after.clone();
            let mut false_facts = after;
            if let Value::Comparison {
                operands: checked_operands,
                ..
            } = &checked.value
            {
                let affines: Vec<Option<Affine>> =
                    checked_operands.iter().map(|o| affine(o, scope)).collect();
                for k in 0..operators.len() {
                    if let (Some(l), Some(r)) = (&affines[k], &affines[k + 1]) {
                        true_facts.add_comparison(l, operators[k], r, false, locations[k]);
                    }
                    // 比較連鎖の後半オペランドに副作用があれば、直前の比較で覚えた項のうち変わるものを忘れる。
                    for later in &ast_operands[k + 1..] {
                        forget_expr_writes_into(later, scope, &mut true_facts);
                    }
                }
                if operators.len() == 1
                    && let (Some(l), Some(r)) = (&affines[0], &affines[1])
                {
                    false_facts.add_comparison(l, operators[0], r, true, locations[0]);
                    forget_expr_writes_into(&ast_operands[1], scope, &mut false_facts);
                }
            }
            Ok((checked, true_facts, false_facts))
        }
        _ => {
            let checked = super::super::expression::expression(expr, None, scope)?;
            let after = scope.array_bounds.facts.borrow().clone();
            Ok((checked, after.clone(), after))
        }
    }
}

/// ブロックが必ず return / fail / break / continue / panic で抜けるかを判定する。
pub(in crate::check) fn block_exits(statements: &[CheckedStatement]) -> bool {
    for stmt in statements {
        if statement_exits(stmt) {
            return true;
        }
    }
    false
}

fn statement_exits(stmt: &CheckedStatement) -> bool {
    match stmt {
        CheckedStatement::Return { .. }
        | CheckedStatement::Fail(_)
        | CheckedStatement::Break
        | CheckedStatement::Continue => true,
        CheckedStatement::Expression(e) => matches!(e.value, Value::Panic { .. }),
        CheckedStatement::Block(body)
        | CheckedStatement::Arena { body, .. }
        | CheckedStatement::Objection(body) => block_exits(body),
        CheckedStatement::If {
            branches,
            otherwise: Some(otherwise),
        } => branches.iter().all(|(_, b)| block_exits(b)) && block_exits(otherwise),
        CheckedStatement::Match { arms, .. } => {
            !arms.is_empty() && arms.iter().all(|(_, b)| block_exits(b))
        }
        _ => false,
    }
}
