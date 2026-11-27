//! 制御の流れの中で判定済みの整数・配列寸法の条件を疎な差分制約グラフ（u - v <= c）として保持し、
//! 推移・定数加減算・合流・忘却と関連診断の由来位置を管理する。

use crate::ast::{BinaryOperator, ComparisonOperator, Location, Type};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    rc::Rc,
};

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
    /// 正規化済み比較。`invertible` が真（整数比較）のときのみ `not` で逆向きの比較へ反転できる。
    Compare {
        op: ComparisonOperator,
        left: Box<PredExpr>,
        right: Box<PredExpr>,
        invertible: bool,
    },
    Or(Box<PredExpr>, Box<PredExpr>),
    Cast(Box<PredExpr>, Type),
}

impl PredExpr {
    pub fn compare(
        op: ComparisonOperator,
        left: PredExpr,
        right: PredExpr,
        invertible: bool,
    ) -> Self {
        match op {
            ComparisonOperator::Greater => Self::Compare {
                op: ComparisonOperator::Less,
                left: Box::new(right),
                right: Box::new(left),
                invertible,
            },
            ComparisonOperator::GreaterEqual => Self::Compare {
                op: ComparisonOperator::LessEqual,
                left: Box::new(right),
                right: Box::new(left),
                invertible,
            },
            ComparisonOperator::Equal | ComparisonOperator::NotEqual => {
                if left <= right {
                    Self::Compare {
                        op,
                        left: Box::new(left),
                        right: Box::new(right),
                        invertible,
                    }
                } else {
                    Self::Compare {
                        op,
                        left: Box::new(right),
                        right: Box::new(left),
                        invertible,
                    }
                }
            }
            ComparisonOperator::Less | ComparisonOperator::LessEqual => Self::Compare {
                op,
                left: Box::new(left),
                right: Box::new(right),
                invertible,
            },
        }
    }

    pub fn negate(self) -> Self {
        match self {
            Self::Bool(b) => Self::Bool(!b),
            Self::Not(inner) => *inner,
            Self::Compare {
                op,
                left,
                right,
                invertible: true,
            } => {
                let inv = match op {
                    ComparisonOperator::Less => ComparisonOperator::GreaterEqual,
                    ComparisonOperator::LessEqual => ComparisonOperator::Greater,
                    ComparisonOperator::Greater => ComparisonOperator::LessEqual,
                    ComparisonOperator::GreaterEqual => ComparisonOperator::Less,
                    ComparisonOperator::Equal => ComparisonOperator::NotEqual,
                    ComparisonOperator::NotEqual => ComparisonOperator::Equal,
                };
                Self::compare(inv, *left, *right, true)
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
            Self::Binary(_, l, r) | Self::Or(l, r) => {
                l.collect_deps(locals, shared);
                r.collect_deps(locals, shared);
            }
            Self::Compare { left, right, .. } => {
                left.collect_deps(locals, shared);
                right.collect_deps(locals, shared);
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
            Self::Compare {
                op,
                left,
                right,
                invertible,
            } => Self::compare(*op, left.map_locals(f)?, right.map_locals(f)?, *invertible),
            Self::Or(l, r) => Self::Or(Box::new(l.map_locals(f)?), Box::new(r.map_locals(f)?)),
            Self::Cast(t, ty) => Self::Cast(Box::new(t.map_locals(f)?), *ty),
        })
    }
}

/// 判定済みの一般 bool 条件。
#[derive(Clone, PartialEq, Eq, Debug)]
pub(in crate::check) struct Predicate {
    pub locals: Vec<usize>,
    pub depends_on_shared: bool,
    pub location: Location,
}

impl Predicate {
    pub fn new(expr: &PredExpr, location: Location) -> Self {
        let mut locals = Vec::new();
        let mut depends_on_shared = false;
        expr.collect_deps(&mut locals, &mut depends_on_shared);
        Self {
            locals,
            depends_on_shared,
            location,
        }
    }
}

type BoundsCache = BTreeMap<Term, (Option<i128>, Option<i128>)>;

/// O(1) 複製・O(log n) 挿入・検索を提供する永続 AVL 木。
#[derive(Clone, Debug)]
struct PMapNode<K, V> {
    key: K,
    val: V,
    height: u8,
    left: Option<Rc<PMapNode<K, V>>>,
    right: Option<Rc<PMapNode<K, V>>>,
}

#[derive(Clone, Debug)]
struct PMap<K, V> {
    root: Option<Rc<PMapNode<K, V>>>,
}

impl<K, V> Default for PMap<K, V> {
    fn default() -> Self {
        Self { root: None }
    }
}

impl<K: Ord + Clone, V: Clone> PMap<K, V> {
    fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    fn ptr_eq(&self, other: &Self) -> bool {
        match (&self.root, &other.root) {
            (None, None) => true,
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }

    fn get(&self, key: &K) -> Option<&V> {
        let mut cur = self.root.as_deref();
        while let Some(node) = cur {
            match key.cmp(&node.key) {
                std::cmp::Ordering::Less => cur = node.left.as_deref(),
                std::cmp::Ordering::Greater => cur = node.right.as_deref(),
                std::cmp::Ordering::Equal => return Some(&node.val),
            }
        }
        None
    }

    fn insert(&self, key: K, val: V) -> Self {
        Self {
            root: Some(Self::insert_node(self.root.as_ref(), key, val)),
        }
    }

    fn node_height(node: Option<&Rc<PMapNode<K, V>>>) -> i32 {
        node.map_or(0, |n| i32::from(n.height))
    }

    fn make_node(
        key: K,
        val: V,
        left: Option<Rc<PMapNode<K, V>>>,
        right: Option<Rc<PMapNode<K, V>>>,
    ) -> Rc<PMapNode<K, V>> {
        let hl = Self::node_height(left.as_ref());
        let hr = Self::node_height(right.as_ref());
        let height = (hl.max(hr) + 1) as u8;
        Rc::new(PMapNode {
            key,
            val,
            height,
            left,
            right,
        })
    }

    fn balance(
        key: K,
        val: V,
        left: Option<Rc<PMapNode<K, V>>>,
        right: Option<Rc<PMapNode<K, V>>>,
    ) -> Rc<PMapNode<K, V>> {
        let bf = Self::node_height(left.as_ref()) - Self::node_height(right.as_ref());
        if bf > 1 {
            let l = left.expect("left child exists when bf > 1");
            if Self::node_height(l.left.as_ref()) >= Self::node_height(l.right.as_ref()) {
                let new_r = Self::make_node(key, val, l.right.clone(), right);
                Self::make_node(l.key.clone(), l.val.clone(), l.left.clone(), Some(new_r))
            } else {
                let lr = l.right.as_ref().expect("left-right child exists");
                let new_l = Self::make_node(
                    l.key.clone(),
                    l.val.clone(),
                    l.left.clone(),
                    lr.left.clone(),
                );
                let new_r = Self::make_node(key, val, lr.right.clone(), right);
                Self::make_node(lr.key.clone(), lr.val.clone(), Some(new_l), Some(new_r))
            }
        } else if bf < -1 {
            let r = right.expect("right child exists when bf < -1");
            if Self::node_height(r.right.as_ref()) >= Self::node_height(r.left.as_ref()) {
                let new_l = Self::make_node(key, val, left, r.left.clone());
                Self::make_node(r.key.clone(), r.val.clone(), Some(new_l), r.right.clone())
            } else {
                let rl = r.left.as_ref().expect("right-left child exists");
                let new_l = Self::make_node(key, val, left, rl.left.clone());
                let new_r = Self::make_node(
                    r.key.clone(),
                    r.val.clone(),
                    rl.right.clone(),
                    r.right.clone(),
                );
                Self::make_node(rl.key.clone(), rl.val.clone(), Some(new_l), Some(new_r))
            }
        } else {
            Self::make_node(key, val, left, right)
        }
    }

    fn insert_node(node: Option<&Rc<PMapNode<K, V>>>, key: K, val: V) -> Rc<PMapNode<K, V>> {
        let Some(n) = node else {
            return Self::make_node(key, val, None, None);
        };
        match key.cmp(&n.key) {
            std::cmp::Ordering::Less => {
                let new_l = Self::insert_node(n.left.as_ref(), key, val);
                Self::balance(n.key.clone(), n.val.clone(), Some(new_l), n.right.clone())
            }
            std::cmp::Ordering::Greater => {
                let new_r = Self::insert_node(n.right.as_ref(), key, val);
                Self::balance(n.key.clone(), n.val.clone(), n.left.clone(), Some(new_r))
            }
            std::cmp::Ordering::Equal => Self::make_node(key, val, n.left.clone(), n.right.clone()),
        }
    }

    fn for_each(&self, f: &mut impl FnMut(&K, &V)) {
        fn walk<K, V>(node: Option<&Rc<PMapNode<K, V>>>, f: &mut impl FnMut(&K, &V)) {
            if let Some(n) = node {
                walk(n.left.as_ref(), f);
                f(&n.key, &n.val);
                walk(n.right.as_ref(), f);
            }
        }
        walk(self.root.as_ref(), f);
    }
}

#[derive(Clone, Default, Debug)]
struct ExtraFacts {
    predicates: PMap<PredExpr, Predicate>,
    /// 各ローカル変数に依存する述語の個数（未依存の `forget_local` を O(log V) で打ち切る）。
    local_counts: PMap<usize, u32>,
    /// 共有参照先・配列要素に依存する述語の個数（0 なら `forget_shared` を O(1) で打ち切る）。
    shared_count: usize,
    bounds_cache: RefCell<Option<Rc<BoundsCache>>>,
}

impl ExtraFacts {
    fn rebuild_from_kept(
        predicates: &PMap<PredExpr, Predicate>,
        mut keep: impl FnMut(&PredExpr, &Predicate) -> bool,
    ) -> Self {
        let mut out = Self::default();
        predicates.for_each(&mut |expr, pred| {
            if keep(expr, pred) {
                out.insert_predicate(expr.clone(), pred.clone());
            }
        });
        out
    }

    fn insert_predicate(&mut self, expr: PredExpr, meta: Predicate) {
        if meta.depends_on_shared {
            self.shared_count += 1;
        }
        for &id in &meta.locals {
            let next = self.local_counts.get(&id).copied().unwrap_or(0) + 1;
            self.local_counts = self.local_counts.insert(id, next);
        }
        self.predicates = self.predicates.insert(expr, meta);
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
    /// 一般 bool 条件と `Term::Zero` 上下限キャッシュ（1 ポインタにまとめてスタック幅を抑える）。
    extra: Option<Rc<ExtraFacts>>,
}

impl Facts {
    fn invalidate_bounds_cache(&mut self) {
        if let Some(extra) = &mut self.extra
            && extra.bounds_cache.borrow().is_some()
        {
            if extra.predicates.is_empty() {
                self.extra = None;
            } else {
                *Rc::make_mut(extra).bounds_cache.get_mut() = None;
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
            && self.range_locals.is_empty()
            && self
                .extra
                .as_deref()
                .is_none_or(|e| e.predicates.is_empty())
    }

    pub fn clear_edges(&mut self) {
        self.edges.clear();
        self.extra = None;
    }

    pub fn add_predicate(&mut self, expr: PredExpr, location: Location) {
        if matches!(expr, PredExpr::Bool(true)) {
            return;
        }
        if let Some(extra) = &self.extra
            && extra.predicates.get(&expr).is_some()
        {
            return;
        }
        let meta = Predicate::new(&expr, location);
        let extra = Rc::make_mut(
            self.extra
                .get_or_insert_with(|| Rc::new(ExtraFacts::default())),
        );
        extra.insert_predicate(expr, meta);
    }

    pub fn has_predicate(&self, expr: &PredExpr) -> Option<Location> {
        self.extra
            .as_deref()?
            .predicates
            .get(expr)
            .map(|p| p.location)
    }

    /// 共有された参照先・配列要素に依存する条件を忘れる（配列参照の `len` は忘れない）。
    pub fn forget_shared(&mut self) {
        if let Some(extra) = &mut self.extra
            && extra.shared_count > 0
        {
            let rebuilt =
                ExtraFacts::rebuild_from_kept(&extra.predicates, |_, p| !p.depends_on_shared);
            let cache = extra.bounds_cache.borrow().clone();
            if rebuilt.predicates.is_empty() && cache.is_none() {
                self.extra = None;
            } else {
                *rebuilt.bounds_cache.borrow_mut() = cache;
                *extra = Rc::new(rebuilt);
            }
        }
    }

    pub fn mark_nonneg(&mut self, id: usize) {
        if !self.nonneg_locals.contains(&id) {
            self.nonneg_locals.push(id);
            self.invalidate_bounds_cache();
        }
    }

    pub fn mark_range(&mut self, id: usize, lo: i128, hi: i128) {
        if let Some(entry) = self.range_locals.iter_mut().find(|(x, _, _)| *x == id) {
            entry.1 = lo;
            entry.2 = hi;
        } else {
            self.range_locals.push((id, lo, hi));
        }
        self.invalidate_bounds_cache();
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
                self.invalidate_bounds_cache();
            }
        } else {
            self.edges.push((u, v, Edge { bound, origins }));
            self.invalidate_bounds_cache();
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

    /// `term` の証明済み定数上下限 `(min, max)` を返す（キャッシュを使って O(log V) で応答する）。
    pub fn proven_bounds(&self, term: &Term) -> (Option<i128>, Option<i128>) {
        if *term == Term::Zero {
            return (Some(0), Some(0));
        }
        let mut base_lo = self
            .range_of(term)
            .map(|(l, _)| l)
            .or_else(|| self.is_implicit_nonneg(term).then_some(0));
        let mut base_hi = self.range_of(term).map(|(_, h)| h);
        if self.edges.is_empty() {
            return (base_lo, base_hi);
        }
        // `term` が非 Zero 間の関係辺（u != Zero && v != Zero）に一度も現れなければ、
        // Zero との直接辺だけで厳密な上下限が決まるため SPFA を完全に省ける。
        let mut term_in_relational = false;
        let mut has_zero_anchor = !self.nonneg_locals.is_empty() || !self.range_locals.is_empty();
        for (u, v, e) in &self.edges {
            if *u == Term::Zero {
                has_zero_anchor = true;
                if v == term
                    && let Some(lo) = e.bound.checked_neg()
                {
                    base_lo = Some(base_lo.map_or(lo, |cur| cur.max(lo)));
                }
            } else if *v == Term::Zero {
                has_zero_anchor = true;
                if u == term {
                    base_hi = Some(base_hi.map_or(e.bound, |cur| cur.min(e.bound)));
                }
            } else if u == term || v == term {
                term_in_relational = true;
            }
        }
        if !term_in_relational || !has_zero_anchor {
            return (base_lo, base_hi);
        }
        let cache = if let Some(extra) = &self.extra {
            let mut slot = extra.bounds_cache.borrow_mut();
            if let Some(existing) = slot.as_ref() {
                existing.clone()
            } else {
                let computed = Rc::new(self.compute_all_bounds());
                *slot = Some(computed.clone());
                computed
            }
        } else {
            Rc::new(self.compute_all_bounds())
        };
        let (c_lo, c_hi) = cache.get(term).copied().unwrap_or((None, None));
        let lo = match (base_lo, c_lo) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        let hi = match (base_hi, c_hi) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        (lo, hi)
    }

    /// `Term::Zero` からの順方向・逆方向 SPFA 各 1 回で全頂点の定数上下限を一括計算する。
    fn compute_all_bounds(&self) -> BoundsCache {
        let (terms, adj) = self.adjacency();
        let n = terms.len();
        let Ok(zero_i) = terms.binary_search(&Term::Zero) else {
            return BoundsCache::new();
        };
        // 順方向: 0 - term <= d  <=>  term >= -d
        let from_zero = Self::shortest_from(zero_i, n, &adj);
        // 逆方向グラフ: term -> 0 を 0 -> term へ反転して SPFA 1 回で全頂点の term - 0 <= d を求める。
        let mut rev_adj: Vec<Vec<(usize, Edge)>> = vec![Vec::new(); n];
        for (u, edges) in adj.iter().enumerate() {
            for (v, edge) in edges {
                rev_adj[*v].push((u, edge.clone()));
            }
        }
        let to_zero = Self::shortest_from(zero_i, n, &rev_adj);
        let mut out = BoundsCache::new();
        for (i, term) in terms.into_iter().enumerate() {
            if i == zero_i {
                continue;
            }
            let lo = from_zero[i].as_ref().and_then(|e| e.bound.checked_neg());
            let hi = to_zero[i].as_ref().map(|e| e.bound);
            if lo.is_some() || hi.is_some() {
                out.insert(term, (lo, hi));
            }
        }
        out
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
                true,
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
        self.invalidate_bounds_cache();
    }

    /// ローカル変数 `id`（およびそれを根とするフィールド経路）に関する条件を忘れる。
    pub fn forget_local(&mut self, id: usize) {
        if let Some(extra) = &mut self.extra
            && extra.local_counts.get(&id).is_some()
        {
            let rebuilt =
                ExtraFacts::rebuild_from_kept(&extra.predicates, |_, p| !p.locals.contains(&id));
            let cache = extra.bounds_cache.borrow().clone();
            if rebuilt.predicates.is_empty() && cache.is_none() {
                self.extra = None;
            } else {
                *rebuilt.bounds_cache.borrow_mut() = cache;
                *extra = Rc::new(rebuilt);
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
        if let Some(extra) = &mut self.extra
            && extra.local_counts.get(&root_id).is_some()
        {
            let rebuilt = ExtraFacts::rebuild_from_kept(&extra.predicates, |_, p| {
                !p.locals.contains(&root_id)
            });
            let cache = extra.bounds_cache.borrow().clone();
            if rebuilt.predicates.is_empty() && cache.is_none() {
                self.extra = None;
            } else {
                *rebuilt.bounds_cache.borrow_mut() = cache;
                *extra = Rc::new(rebuilt);
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
        let extra = first.extra.as_ref().and_then(|first_rc| {
            if first_rc.predicates.is_empty() {
                return None;
            }
            if rest.iter().all(|p| {
                p.extra
                    .as_ref()
                    .is_some_and(|e| e.predicates.ptr_eq(&first_rc.predicates))
            }) {
                return Some(first_rc.clone());
            }
            let rebuilt = ExtraFacts::rebuild_from_kept(&first_rc.predicates, |expr, _| {
                rest.iter().all(|p| p.has_predicate(expr).is_some())
            });
            (!rebuilt.predicates.is_empty()).then(|| Rc::new(rebuilt))
        });
        if paths.iter().any(|f| f.edges.is_empty()) {
            return Facts {
                edges: Vec::new(),
                nonneg_locals,
                range_locals,
                extra,
            };
        }
        let mut joined = Facts {
            edges: Vec::new(),
            nonneg_locals,
            range_locals,
            extra,
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
        if let Some(extra) = &self.extra {
            extra.predicates.for_each(&mut |expr, pred| {
                if pred.depends_on_shared {
                    return;
                }
                if let Some(mapped_expr) = expr.map_locals(&mut |id| {
                    if let Some(Term::Local(new_id)) = map_term(&Term::Local(id)) {
                        Some(PredExpr::Local(new_id))
                    } else {
                        None
                    }
                }) {
                    out.add_predicate(mapped_expr, pred.location);
                }
            });
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
