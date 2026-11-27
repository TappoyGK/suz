//! 制御の流れの中で判定済みの整数・配列寸法の条件を疎な差分制約グラフ（u - v <= c）として保持し、
//! 推移・定数加減算・合流・忘却と関連診断の由来位置を管理する。

use crate::ast::{ComparisonOperator, Location};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

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
}

impl Facts {
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty() && self.range_locals.is_empty()
    }

    pub fn clear_edges(&mut self) {
        self.edges.clear();
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
        if paths.iter().any(|f| f.edges.is_empty()) {
            return Facts {
                edges: Vec::new(),
                nonneg_locals,
                range_locals,
            };
        }
        let mut joined = Facts {
            edges: Vec::new(),
            nonneg_locals,
            range_locals,
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
