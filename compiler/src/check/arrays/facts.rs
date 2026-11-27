//! 制御の流れの中で判定済みの整数・配列寸法の条件を疎な差分制約グラフ（u - v <= c）として保持し、
//! 推移・定数加減算・合流・忘却と関連診断の由来位置を管理する。

use crate::ast::{BinaryOperator, ComparisonOperator, Location, Type};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// 一般の bool 条件を正規化して保持するための式表現。
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(in crate::check) enum PredExpr {
    Bool(bool),
    Int(i128),
    FloatBits(u64),
    Char(u32),
    Symbol(String),
    Nil,
    Local(usize),
    Field(Box<PredExpr>, usize),
    Deref(Box<PredExpr>),
    Len(Box<PredExpr>, usize),
    Index(Box<PredExpr>, Vec<PredExpr>),
    Not(Box<PredExpr>),
    Neg(Box<PredExpr>),
    Binary(BinaryOperator, Box<PredExpr>, Box<PredExpr>),
    Compare(ComparisonOperator, Box<PredExpr>, Box<PredExpr>),
    Or(Box<PredExpr>, Box<PredExpr>),
    Cast(Box<PredExpr>, Type),
}

impl PredExpr {
    pub fn compare(op: ComparisonOperator, left: PredExpr, right: PredExpr) -> Self {
        match op {
            ComparisonOperator::Greater => {
                Self::Compare(ComparisonOperator::Less, Box::new(right), Box::new(left))
            }
            ComparisonOperator::GreaterEqual => Self::Compare(
                ComparisonOperator::LessEqual,
                Box::new(right),
                Box::new(left),
            ),
            ComparisonOperator::Equal | ComparisonOperator::NotEqual => {
                if left <= right {
                    Self::Compare(op, Box::new(left), Box::new(right))
                } else {
                    Self::Compare(op, Box::new(right), Box::new(left))
                }
            }
            ComparisonOperator::Less | ComparisonOperator::LessEqual => {
                Self::Compare(op, Box::new(left), Box::new(right))
            }
        }
    }

    pub fn negate(self) -> Self {
        match self {
            Self::Bool(b) => Self::Bool(!b),
            Self::Not(inner) => *inner,
            Self::Compare(op, l, r) => {
                let inv = match op {
                    ComparisonOperator::Less => ComparisonOperator::GreaterEqual,
                    ComparisonOperator::LessEqual => ComparisonOperator::Greater,
                    ComparisonOperator::Greater => ComparisonOperator::LessEqual,
                    ComparisonOperator::GreaterEqual => ComparisonOperator::Less,
                    ComparisonOperator::Equal => ComparisonOperator::NotEqual,
                    ComparisonOperator::NotEqual => ComparisonOperator::Equal,
                };
                Self::compare(inv, *l, *r)
            }
            other => Self::Not(Box::new(other)),
        }
    }

    pub fn collect_deps(&self, locals: &mut Vec<usize>, shared: &mut bool) {
        match self {
            Self::Bool(_)
            | Self::Int(_)
            | Self::FloatBits(_)
            | Self::Char(_)
            | Self::Symbol(_)
            | Self::Nil => {}
            Self::Local(id) => {
                if !locals.contains(id) {
                    locals.push(*id);
                }
            }
            Self::Field(target, _)
            | Self::Not(target)
            | Self::Neg(target)
            | Self::Cast(target, _) => {
                target.collect_deps(locals, shared);
            }
            Self::Len(target, _) => {
                // 配列参照の len は配列要素の内容には依存しない（ポインタ経由で配列参照自体を読む場合のみ shared）。
                target.collect_deps(locals, shared);
            }
            Self::Deref(target) => {
                *shared = true;
                target.collect_deps(locals, shared);
            }
            Self::Index(target, indices) => {
                *shared = true;
                target.collect_deps(locals, shared);
                for idx in indices {
                    idx.collect_deps(locals, shared);
                }
            }
            Self::Binary(_, l, r) | Self::Compare(_, l, r) | Self::Or(l, r) => {
                l.collect_deps(locals, shared);
                r.collect_deps(locals, shared);
            }
        }
    }

    pub fn map_locals(&self, f: &mut impl FnMut(usize) -> Option<PredExpr>) -> Option<PredExpr> {
        Some(match self {
            Self::Bool(b) => Self::Bool(*b),
            Self::Int(n) => Self::Int(*n),
            Self::FloatBits(b) => Self::FloatBits(*b),
            Self::Char(c) => Self::Char(*c),
            Self::Symbol(s) => Self::Symbol(s.clone()),
            Self::Nil => Self::Nil,
            Self::Local(id) => f(*id)?,
            Self::Field(t, idx) => Self::Field(Box::new(t.map_locals(f)?), *idx),
            Self::Deref(t) => Self::Deref(Box::new(t.map_locals(f)?)),
            Self::Len(t, dim) => Self::Len(Box::new(t.map_locals(f)?), *dim),
            Self::Index(t, indices) => {
                let t = Box::new(t.map_locals(f)?);
                let mut out = Vec::with_capacity(indices.len());
                for idx in indices {
                    out.push(idx.map_locals(f)?);
                }
                Self::Index(t, out)
            }
            Self::Not(t) => t.map_locals(f)?.negate(),
            Self::Neg(t) => Self::Neg(Box::new(t.map_locals(f)?)),
            Self::Binary(op, l, r) => {
                Self::Binary(*op, Box::new(l.map_locals(f)?), Box::new(r.map_locals(f)?))
            }
            Self::Compare(op, l, r) => Self::compare(*op, l.map_locals(f)?, r.map_locals(f)?),
            Self::Or(l, r) => Self::Or(Box::new(l.map_locals(f)?), Box::new(r.map_locals(f)?)),
            Self::Cast(t, ty) => Self::Cast(Box::new(t.map_locals(f)?), *ty),
        })
    }
}

/// 判定済みの一般 bool 条件。
#[derive(Clone, PartialEq, Eq, Debug)]
pub(in crate::check) struct Predicate {
    pub expr: PredExpr,
    pub locals: Vec<usize>,
    pub depends_on_shared: bool,
    pub location: Location,
}

impl Predicate {
    pub fn new(expr: PredExpr, location: Location) -> Self {
        let mut locals = Vec::new();
        let mut depends_on_shared = false;
        expr.collect_deps(&mut locals, &mut depends_on_shared);
        Self {
            expr,
            locals,
            depends_on_shared,
            location,
        }
    }
}

/// 差分制約グラフの頂点。
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(in crate::check) enum Term {
    /// 定数 0。
    Zero,
    /// ローカルの整数束縛（fix / var / 仮引数 / ループ変数）。
    Local(usize),
    /// 配列の寸法（fix 配列・仮引数・寸法名・別名で共有する由来 (source_id, axis)）。
    Dim(usize, usize),
    /// var 配列（fields が空）または構造体フィールド経路の配列寸法 (root_local_id, field_indices, axis)。
    PathDim(usize, Vec<usize>, usize),
}

/// 項 + 定数オフセット（`term + offset`）。
#[derive(Clone, PartialEq, Eq, Debug)]
pub(in crate::check) struct Affine {
    pub term: Term,
    pub offset: i128,
}

impl Affine {
    pub fn constant(value: i128) -> Self {
        Self {
            term: Term::Zero,
            offset: value,
        }
    }

    pub fn term(term: Term) -> Self {
        Self { term, offset: 0 }
    }

    pub fn shifted(self, delta: i128) -> Option<Self> {
        Some(Self {
            term: self.term,
            offset: self.offset.checked_add(delta)?,
        })
    }
}

/// 制約の根拠となった条件のソース位置群。0〜1 件ではヒープ確保を避ける。
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(in crate::check) enum Origins {
    #[default]
    Empty,
    One(Location),
    Many(Vec<Location>),
}

impl Origins {
    pub fn from_vec(mut v: Vec<Location>) -> Self {
        v.sort_by_key(|l| (l.line, l.column));
        v.dedup();
        match v.len() {
            0 => Self::Empty,
            1 => Self::One(v[0]),
            _ => Self::Many(v),
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub fn to_vec(&self) -> Vec<Location> {
        match self {
            Self::Empty => Vec::new(),
            Self::One(l) => vec![*l],
            Self::Many(v) => v.clone(),
        }
    }

    fn merge(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Empty, b) => b.clone(),
            (a, Self::Empty) => a.clone(),
            (Self::One(a), Self::One(b)) if a == b => Self::One(*a),
            (Self::One(a), Self::One(b)) => {
                if (a.line, a.column) <= (b.line, b.column) {
                    Self::Many(vec![*a, *b])
                } else {
                    Self::Many(vec![*b, *a])
                }
            }
            _ => {
                let mut out = self.to_vec();
                out.extend(other.to_vec());
                Self::from_vec(out)
            }
        }
    }
}

/// 制約 `u - v <= bound` と、その根拠となった条件のソース位置群。
#[derive(Clone, PartialEq, Eq, Debug)]
struct Edge {
    bound: i128,
    origins: Origins,
}

/// 判定済みの条件集合（疎グラフ表現）。
#[derive(Clone, Default, Debug)]
pub(in crate::check) struct Facts {
    edges: Vec<(Term, Term, Edge)>,
    /// 符号なし整数型・`len_t` のローカル束縛（常に 0 以上であり、代入でも失われない）。
    nonneg_locals: Vec<usize>,
    /// 値域制限型のローカル束縛（常に `lo <= x <= hi` であり、代入でも失われない）。
    range_locals: Vec<(usize, i128, i128)>,
    /// 判定済みの一般 bool 条件（構文一致・`&&` 分解・比較反転用）。空のときは確保しない。
    #[allow(clippy::box_collection)]
    predicates: Option<Box<Vec<Predicate>>>,
}

impl Facts {
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
            && self.range_locals.is_empty()
            && self.predicates.as_deref().is_none_or(Vec::is_empty)
    }

    pub fn clear_edges(&mut self) {
        self.edges.clear();
        self.predicates = None;
    }

    pub fn add_predicate(&mut self, expr: PredExpr, location: Location) {
        if matches!(expr, PredExpr::Bool(true)) {
            return;
        }
        let preds = self.predicates.get_or_insert_with(|| Box::new(Vec::new()));
        if !preds.iter().any(|p| p.expr == expr) {
            preds.push(Predicate::new(expr, location));
        }
    }

    pub fn has_predicate(&self, expr: &PredExpr) -> Option<Location> {
        self.predicates
            .as_deref()?
            .iter()
            .find(|p| &p.expr == expr)
            .map(|p| p.location)
    }

    /// 共有された参照先・配列要素に依存する条件を忘れる（配列参照の `len` は忘れない）。
    pub fn forget_shared(&mut self) {
        if let Some(preds) = &mut self.predicates {
            preds.retain(|p| !p.depends_on_shared);
            if preds.is_empty() {
                self.predicates = None;
            }
        }
    }

    pub fn mark_nonneg(&mut self, id: usize) {
        if !self.nonneg_locals.contains(&id) {
            self.nonneg_locals.push(id);
        }
    }

    pub fn mark_range(&mut self, id: usize, lo: i128, hi: i128) {
        if let Some(entry) = self.range_locals.iter_mut().find(|(x, _, _)| *x == id) {
            entry.1 = lo;
            entry.2 = hi;
        } else {
            self.range_locals.push((id, lo, hi));
        }
        if lo >= 0 {
            self.mark_nonneg(id);
        }
    }

    fn range_of(&self, term: &Term) -> Option<(i128, i128)> {
        let Term::Local(id) = term else {
            return None;
        };
        self.range_locals
            .iter()
            .find(|(x, _, _)| *x == *id)
            .map(|&(_, lo, hi)| (lo, hi))
    }

    pub fn unmark_nonneg(&mut self, id: usize) {
        if self.nonneg_locals.last() == Some(&id) {
            self.nonneg_locals.pop();
        } else if let Some(pos) = self.nonneg_locals.iter().position(|x| *x == id) {
            self.nonneg_locals.swap_remove(pos);
        }
    }

    fn is_implicit_nonneg(&self, term: &Term) -> bool {
        match term {
            Term::Dim(..) | Term::PathDim(..) => true,
            Term::Local(id) => self.nonneg_locals.contains(id),
            Term::Zero => false,
        }
    }

    fn get_edge(&self, u: &Term, v: &Term) -> Option<&Edge> {
        self.edges
            .iter()
            .find(|(eu, ev, _)| eu == u && ev == v)
            .map(|(_, _, e)| e)
    }

    /// `u - v <= bound` を記録する。既存の制約より厳しい場合のみ更新する。
    pub fn add_diff(&mut self, u: Term, v: Term, bound: i128, origins: Origins) {
        if u == v {
            return;
        }
        if u == Term::Zero && self.is_implicit_nonneg(&v) && bound >= 0 && origins.is_empty() {
            return;
        }
        if let Some((_, _, existing)) = self
            .edges
            .iter_mut()
            .find(|(eu, ev, _)| *eu == u && *ev == v)
        {
            if bound < existing.bound {
                existing.bound = bound;
                existing.origins = origins;
            }
        } else {
            self.edges.push((u, v, Edge { bound, origins }));
        }
    }

    /// `left op right`（`negated` なら否定）を記録する。
    pub fn add_comparison(
        &mut self,
        left: &Affine,
        op: ComparisonOperator,
        right: &Affine,
        negated: bool,
        location: Location,
    ) {
        let op = if negated {
            match op {
                ComparisonOperator::Less => ComparisonOperator::GreaterEqual,
                ComparisonOperator::LessEqual => ComparisonOperator::Greater,
                ComparisonOperator::Greater => ComparisonOperator::LessEqual,
                ComparisonOperator::GreaterEqual => ComparisonOperator::Less,
                ComparisonOperator::Equal => return,
                ComparisonOperator::NotEqual => ComparisonOperator::Equal,
            }
        } else {
            op
        };
        let origins = Origins::One(location);
        match op {
            ComparisonOperator::Less => {
                if let Some(b) = right
                    .offset
                    .checked_sub(left.offset)
                    .and_then(|d| d.checked_sub(1))
                {
                    self.add_diff(left.term.clone(), right.term.clone(), b, origins);
                }
            }
            ComparisonOperator::LessEqual => {
                if let Some(b) = right.offset.checked_sub(left.offset) {
                    self.add_diff(left.term.clone(), right.term.clone(), b, origins);
                }
            }
            ComparisonOperator::Greater => {
                if let Some(b) = left
                    .offset
                    .checked_sub(right.offset)
                    .and_then(|d| d.checked_sub(1))
                {
                    self.add_diff(right.term.clone(), left.term.clone(), b, origins);
                }
            }
            ComparisonOperator::GreaterEqual => {
                if let Some(b) = left.offset.checked_sub(right.offset) {
                    self.add_diff(right.term.clone(), left.term.clone(), b, origins);
                }
            }
            ComparisonOperator::Equal => {
                if let Some(b) = right.offset.checked_sub(left.offset) {
                    self.add_diff(left.term.clone(), right.term.clone(), b, origins.clone());
                }
                if let Some(b) = left.offset.checked_sub(right.offset) {
                    self.add_diff(right.term.clone(), left.term.clone(), b, origins);
                }
            }
            ComparisonOperator::NotEqual => {}
        }
    }

    /// 疎グラフの隣接リスト（`u -> [(v, Edge)]`）を構築する。
    fn adjacency(&self) -> (Vec<Term>, Vec<Vec<(usize, Edge)>>) {
        let mut terms_set = BTreeSet::new();
        terms_set.insert(Term::Zero);
        for (u, v, _) in &self.edges {
            terms_set.insert(u.clone());
            terms_set.insert(v.clone());
        }
        for &(id, _, _) in &self.range_locals {
            terms_set.insert(Term::Local(id));
        }
        let terms: Vec<Term> = terms_set.into_iter().collect();
        let n = terms.len();
        let mut adj: Vec<Vec<(usize, Edge)>> = vec![Vec::new(); n];
        let index_of = |t: &Term| terms.binary_search(t).unwrap();
        let zero_idx = index_of(&Term::Zero);
        for (i, term) in terms.iter().enumerate() {
            if let Some((lo, hi)) = self.range_of(term) {
                if let Some(neg_lo) = lo.checked_neg() {
                    adj[zero_idx].push((
                        i,
                        Edge {
                            bound: neg_lo,
                            origins: Origins::Empty,
                        },
                    ));
                }
                adj[i].push((
                    zero_idx,
                    Edge {
                        bound: hi,
                        origins: Origins::Empty,
                    },
                ));
            } else if self.is_implicit_nonneg(term) {
                adj[zero_idx].push((
                    i,
                    Edge {
                        bound: 0,
                        origins: Origins::Empty,
                    },
                ));
            }
        }
        for (u, v, edge) in &self.edges {
            let ui = index_of(u);
            let vi = index_of(v);
            adj[ui].push((vi, edge.clone()));
        }
        (terms, adj)
    }

    /// 単一始点最短路（SPFA、緩和回数上限で負閉路でも必ず停止）。
    fn shortest_from(start: usize, n: usize, adj: &[Vec<(usize, Edge)>]) -> Vec<Option<Edge>> {
        let mut dist: Vec<Option<Edge>> = vec![None; n];
        let mut in_queue = vec![false; n];
        let mut relax_count = vec![0usize; n];
        let mut queue = VecDeque::new();
        dist[start] = Some(Edge {
            bound: 0,
            origins: Origins::Empty,
        });
        queue.push_back(start);
        in_queue[start] = true;
        while let Some(u) = queue.pop_front() {
            in_queue[u] = false;
            let Some(du) = dist[u].clone() else {
                continue;
            };
            for (v, edge) in &adj[u] {
                let Some(cand) = du.bound.checked_add(edge.bound) else {
                    continue;
                };
                let better = match &dist[*v] {
                    None => true,
                    Some(cur) => cand < cur.bound,
                };
                if better && relax_count[*v] < n {
                    dist[*v] = Some(Edge {
                        bound: cand,
                        origins: du.origins.merge(&edge.origins),
                    });
                    relax_count[*v] += 1;
                    if !in_queue[*v] {
                        queue.push_back(*v);
                        in_queue[*v] = true;
                    }
                }
            }
        }
        dist
    }

    /// `left <= right` が導けるなら、その根拠となった条件のソース位置を返す。
    pub fn proves_le(&self, left: &Affine, right: &Affine) -> Option<Vec<Location>> {
        let needed = right.offset.checked_sub(left.offset)?;
        if left.term == right.term {
            return (0 <= needed).then(Vec::new);
        }
        if left.term == Term::Zero && self.is_implicit_nonneg(&right.term) && 0 <= needed {
            return Some(Vec::new());
        }
        if left.term == Term::Zero
            && let Some((lo, _)) = self.range_of(&right.term)
            && let Some(neg_lo) = lo.checked_neg()
            && neg_lo <= needed
        {
            return Some(Vec::new());
        }
        if right.term == Term::Zero
            && let Some((_, hi)) = self.range_of(&left.term)
            && hi <= needed
        {
            return Some(Vec::new());
        }
        if self.edges.is_empty() && self.range_locals.is_empty() {
            return None;
        }
        // 直接辺で満たせる場合は探索を省く。
        if let Some(edge) = self.get_edge(&left.term, &right.term)
            && edge.bound <= needed
        {
            return Some(edge.origins.to_vec());
        }
        let (terms, adj) = self.adjacency();
        let ui = terms.binary_search(&left.term).ok()?;
        let vi = terms.binary_search(&right.term).ok()?;
        let dist = Self::shortest_from(ui, terms.len(), &adj);
        let edge = dist[vi].as_ref()?;
        (edge.bound <= needed).then(|| edge.origins.to_vec())
    }

    /// `left < right` が導けるなら、その根拠となった条件のソース位置を返す。
    pub fn proves_lt(&self, left: &Affine, right: &Affine) -> Option<Vec<Location>> {
        let shifted = right.clone().shifted(-1)?;
        self.proves_le(left, &shifted)
    }

    /// `left == right` が導けるなら、その根拠となった条件のソース位置を返す。
    pub fn proves_eq(&self, left: &Affine, right: &Affine) -> Option<Vec<Location>> {
        let mut a = self.proves_le(left, right)?;
        let b = self.proves_le(right, left)?;
        a.extend(b);
        a.sort_by_key(|l| (l.line, l.column));
        a.dedup();
        Some(a)
    }

    /// `term` の証明済み定数上下限 `(min, max)` を返す（`edges` が空なら高速に返す）。
    pub fn proven_bounds(&self, term: &Term) -> (Option<i128>, Option<i128>) {
        if *term == Term::Zero {
            return (Some(0), Some(0));
        }
        let mut lo = self
            .range_of(term)
            .map(|(l, _)| l)
            .or_else(|| self.is_implicit_nonneg(term).then_some(0));
        let mut hi = self.range_of(term).map(|(_, h)| h);
        if self.edges.is_empty() {
            return (lo, hi);
        }
        let (terms, adj) = self.adjacency();
        let Ok(zero_i) = terms.binary_search(&Term::Zero) else {
            return (lo, hi);
        };
        let Ok(ti) = terms.binary_search(term) else {
            return (lo, hi);
        };
        // 0 - term <= bound  <=>  term >= -bound
        let from_zero = Self::shortest_from(zero_i, terms.len(), &adj);
        if let Some(edge) = &from_zero[ti]
            && let Some(bound_lo) = edge.bound.checked_neg()
        {
            lo = Some(lo.map_or(bound_lo, |cur| cur.max(bound_lo)));
        }
        // term - 0 <= bound  <=>  term <= bound
        let from_term = Self::shortest_from(ti, terms.len(), &adj);
        if let Some(edge) = &from_term[zero_i] {
            hi = Some(hi.map_or(edge.bound, |cur| cur.min(edge.bound)));
        }
        (lo, hi)
    }

    /// `term + offset != 0` が導けるかを返す。
    pub fn proves_nonzero(&self, affine: &Affine) -> bool {
        let (lo, hi) = self.proven_bounds(&affine.term);
        if let Some(l) = lo.and_then(|v| v.checked_add(affine.offset))
            && l > 0
        {
            return true;
        }
        if let Some(h) = hi.and_then(|v| v.checked_add(affine.offset))
            && h < 0
        {
            return true;
        }
        if affine.offset == 0
            && let Term::Local(id) = affine.term
        {
            let target = PredExpr::compare(
                ComparisonOperator::NotEqual,
                PredExpr::Local(id),
                PredExpr::Int(0),
            );
            if self.has_predicate(&target).is_some() {
                return true;
            }
        }
        false
    }

    /// 特定の条件を満たす項を削除する（削除対象ノードのみ局所的に Fourier-Motzkin 消去する）。
    pub fn forget_where(&mut self, mut predicate: impl FnMut(&Term) -> bool) {
        if self.edges.is_empty() {
            return;
        }
        let mut removed = Vec::new();
        for (u, v, _) in &self.edges {
            if predicate(u) && !removed.contains(u) {
                removed.push(u.clone());
            }
            if predicate(v) && !removed.contains(v) {
                removed.push(v.clone());
            }
        }
        if removed.is_empty() {
            return;
        }
        for target in &removed {
            let mut incoming: Vec<(Term, Edge)> = Vec::new();
            let mut outgoing: Vec<(Term, Edge)> = Vec::new();
            for (u, v, edge) in &self.edges {
                if v == target && u != target {
                    incoming.push((u.clone(), edge.clone()));
                } else if u == target && v != target {
                    outgoing.push((v.clone(), edge.clone()));
                }
            }
            if let Some((lo, hi)) = self.range_of(target) {
                if let Some(neg_lo) = lo.checked_neg() {
                    incoming.push((
                        Term::Zero,
                        Edge {
                            bound: neg_lo,
                            origins: Origins::Empty,
                        },
                    ));
                }
                outgoing.push((
                    Term::Zero,
                    Edge {
                        bound: hi,
                        origins: Origins::Empty,
                    },
                ));
            } else if self.is_implicit_nonneg(target) {
                incoming.push((
                    Term::Zero,
                    Edge {
                        bound: 0,
                        origins: Origins::Empty,
                    },
                ));
            }
            for (u, e1) in &incoming {
                if removed.contains(u) {
                    continue;
                }
                for (v, e2) in &outgoing {
                    if removed.contains(v) || u == v {
                        continue;
                    }
                    if let Some(bound) = e1.bound.checked_add(e2.bound) {
                        self.add_diff(u.clone(), v.clone(), bound, e1.origins.merge(&e2.origins));
                    }
                }
            }
            self.edges.retain(|(u, v, _)| u != target && v != target);
        }
    }

    /// ローカル変数 `id`（およびそれを根とするフィールド経路）に関する条件を忘れる。
    pub fn forget_local(&mut self, id: usize) {
        if let Some(preds) = &mut self.predicates {
            preds.retain(|p| !p.locals.contains(&id));
            if preds.is_empty() {
                self.predicates = None;
            }
        }
        self.forget_where(|t| {
            matches!(t, Term::Local(x) if *x == id) || matches!(t, Term::PathDim(x, ..) if *x == id)
        });
    }

    /// `var` 構造体 `root_id` のフィールド経路 `fields` への代入・`#` メソッド呼び出しで影響する条件を忘れる。
    pub fn forget_path(&mut self, root_id: usize, fields: &[usize]) {
        if fields.is_empty() {
            self.forget_local(root_id);
            return;
        }
        if let Some(preds) = &mut self.predicates {
            preds.retain(|p| !p.locals.contains(&root_id));
            if preds.is_empty() {
                self.predicates = None;
            }
        }
        self.forget_where(|t| match t {
            Term::PathDim(x, path, _) if *x == root_id => {
                path.starts_with(fields) || fields.starts_with(path)
            }
            _ => false,
        });
    }

    /// 複数経路の合流：すべての経路で導ける条件のみを残す。
    pub fn join_all(paths: &[Facts]) -> Facts {
        let Some((first, rest)) = paths.split_first() else {
            return Facts::default();
        };
        if rest.is_empty() {
            return first.clone();
        }
        let mut nonneg_locals = first.nonneg_locals.clone();
        for p in rest {
            nonneg_locals.retain(|id| p.nonneg_locals.contains(id));
        }
        let mut range_locals = first.range_locals.clone();
        for p in rest {
            range_locals.retain(|entry| p.range_locals.contains(entry));
        }
        let predicates = first.predicates.as_deref().and_then(|first_preds| {
            let mut kept = first_preds.clone();
            for p in rest {
                kept.retain(|pred| p.has_predicate(&pred.expr).is_some());
            }
            (!kept.is_empty()).then(|| Box::new(kept))
        });
        if paths.iter().any(|f| f.edges.is_empty()) {
            return Facts {
                edges: Vec::new(),
                nonneg_locals,
                range_locals,
                predicates,
            };
        }
        let mut joined = Facts {
            edges: Vec::new(),
            nonneg_locals,
            range_locals,
            predicates,
        };
        // 全経路で完全に一致する辺は探索せずそのまま引き継ぐ。
        let mut changed_pairs: BTreeSet<(Term, Term)> = BTreeSet::new();
        for (u, v, edge) in &first.edges {
            if rest.iter().all(|p| p.get_edge(u, v) == Some(edge)) {
                joined.edges.push((u.clone(), v.clone(), edge.clone()));
            } else {
                changed_pairs.insert((u.clone(), v.clone()));
            }
        }
        for p in rest {
            for (u, v, _) in &p.edges {
                if joined.get_edge(u, v).is_none() {
                    changed_pairs.insert((u.clone(), v.clone()));
                }
            }
        }
        if changed_pairs.is_empty() {
            return joined;
        }
        // 差分のある辺の端点について、全経路の最短距離の上限を取る。
        let mut sources: BTreeSet<Term> = BTreeSet::new();
        for (u, _) in &changed_pairs {
            sources.insert(u.clone());
        }
        let graphs: Vec<_> = paths.iter().map(Facts::adjacency).collect();
        let mut dist_cache: Vec<BTreeMap<Term, Vec<Option<Edge>>>> =
            vec![BTreeMap::new(); paths.len()];
        for src in &sources {
            for (idx, (terms, adj)) in graphs.iter().enumerate() {
                if let Ok(si) = terms.binary_search(src) {
                    dist_cache[idx].insert(src.clone(), Self::shortest_from(si, terms.len(), adj));
                }
            }
        }
        for (u, v) in &changed_pairs {
            let mut max_bound: Option<i128> = None;
            let mut origins = Origins::Empty;
            let mut all_have = true;
            for (idx, (terms, _)) in graphs.iter().enumerate() {
                let Some(dist) = dist_cache[idx].get(u) else {
                    all_have = false;
                    break;
                };
                let Ok(vi) = terms.binary_search(v) else {
                    all_have = false;
                    break;
                };
                let Some(edge) = &dist[vi] else {
                    all_have = false;
                    break;
                };
                max_bound = Some(max_bound.map_or(edge.bound, |m| m.max(edge.bound)));
                origins = origins.merge(&edge.origins);
            }
            if all_have && let Some(bound) = max_bound {
                joined.add_diff(u.clone(), v.clone(), bound, origins);
            }
        }
        joined
    }

    /// クロージャ境界：値キャプチャーで写像できる項だけからなる条件を引き継ぐ。
    pub fn project_captures(&self, mut map_term: impl FnMut(&Term) -> Option<Term>) -> Facts {
        let mut out = Facts::default();
        for &id in &self.nonneg_locals {
            if let Some(Term::Local(new_id)) = map_term(&Term::Local(id)) {
                out.mark_nonneg(new_id);
            }
        }
        for &(id, lo, hi) in &self.range_locals {
            if let Some(Term::Local(new_id)) = map_term(&Term::Local(id)) {
                out.mark_range(new_id, lo, hi);
            }
        }
        if let Some(preds) = &self.predicates {
            for pred in preds.iter() {
                if pred.depends_on_shared {
                    continue;
                }
                if let Some(mapped_expr) = pred.expr.map_locals(&mut |id| {
                    if let Some(Term::Local(new_id)) = map_term(&Term::Local(id)) {
                        Some(PredExpr::Local(new_id))
                    } else {
                        None
                    }
                }) {
                    out.add_predicate(mapped_expr, pred.location);
                }
            }
        }
        if self.edges.is_empty() {
            return out;
        }
        let (terms, adj) = self.adjacency();
        let mapped: Vec<Option<Term>> = terms.iter().map(&mut map_term).collect();
        for i in 0..terms.len() {
            let Some(u) = &mapped[i] else {
                continue;
            };
            let dist = Self::shortest_from(i, terms.len(), &adj);
            for j in 0..terms.len() {
                if i == j {
                    continue;
                }
                let Some(v) = &mapped[j] else {
                    continue;
                };
                if u == v {
                    continue;
                }
                if let Some(edge) = &dist[j] {
                    out.add_diff(u.clone(), v.clone(), edge.bound, edge.origins.clone());
                }
            }
        }
        out
    }
}
