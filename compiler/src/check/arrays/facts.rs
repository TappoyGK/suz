//! 制御の流れの中で判定済みの整数・配列寸法の条件を差分制約（u - v <= c）として保持し、
//! 推移・定数加減算・合流・忘却と関連診断の由来位置を管理する。

use crate::ast::{ComparisonOperator, Location};
use std::collections::{BTreeMap, BTreeSet};

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

/// 制約 `u - v <= bound` と、その根拠となった条件のソース位置群。
#[derive(Clone, Debug)]
struct Edge {
    bound: i128,
    origins: Vec<Location>,
}

/// 判定済みの条件集合。
#[derive(Clone, Default, Debug)]
pub(in crate::check) struct Facts {
    edges: BTreeMap<(Term, Term), Edge>,
}

fn merge_origins(a: &[Location], b: &[Location]) -> Vec<Location> {
    let mut out: Vec<Location> = a.iter().chain(b.iter()).copied().collect();
    out.sort_by_key(|l| (l.line, l.column));
    out.dedup();
    out
}

impl Facts {
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// `u - v <= bound` を記録する。既存の制約より厳しい場合のみ更新する。
    pub fn add_diff(&mut self, u: Term, v: Term, bound: i128, origins: Vec<Location>) {
        if u == v {
            return;
        }
        match self.edges.get_mut(&(u.clone(), v.clone())) {
            Some(existing) => {
                if bound < existing.bound {
                    existing.bound = bound;
                    existing.origins = origins;
                }
            }
            None => {
                self.edges.insert((u, v), Edge { bound, origins });
            }
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
        let origins = vec![location];
        match op {
            ComparisonOperator::Less => {
                // left.term + left.offset <= right.term + right.offset - 1
                // left.term - right.term <= right.offset - left.offset - 1
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

    /// 閉包（全対最短路）を計算する。配列寸法は常に 0 以上であることを暗黙に含める。
    fn closure(&self) -> (Vec<Term>, Vec<Vec<Option<Edge>>>) {
        let mut terms_set = BTreeSet::new();
        terms_set.insert(Term::Zero);
        for (u, v) in self.edges.keys() {
            terms_set.insert(u.clone());
            terms_set.insert(v.clone());
        }
        let terms: Vec<Term> = terms_set.into_iter().collect();
        let n = terms.len();
        let mut dist: Vec<Vec<Option<Edge>>> = vec![vec![None; n]; n];
        for (i, row) in dist.iter_mut().enumerate() {
            row[i] = Some(Edge {
                bound: 0,
                origins: Vec::new(),
            });
        }
        let index_of = |t: &Term| terms.binary_search(t).unwrap();
        let zero_idx = index_of(&Term::Zero);
        // 配列の寸法は常に 0 以上（Zero - Dim <= 0）。
        for (i, term) in terms.iter().enumerate() {
            if matches!(term, Term::Dim(..) | Term::PathDim(..)) {
                dist[zero_idx][i] = Some(Edge {
                    bound: 0,
                    origins: Vec::new(),
                });
            }
        }
        for ((u, v), edge) in &self.edges {
            let ui = index_of(u);
            let vi = index_of(v);
            let update = match &dist[ui][vi] {
                None => true,
                Some(cur) => edge.bound < cur.bound,
            };
            if update {
                dist[ui][vi] = Some(edge.clone());
            }
        }
        // Floyd-Warshall（負閉路があっても必ず n^3 で終了する）。
        for k in 0..n {
            for i in 0..n {
                let Some(ik) = dist[i][k].clone() else {
                    continue;
                };
                for j in 0..n {
                    if i == j {
                        continue;
                    }
                    let Some(kj) = &dist[k][j] else {
                        continue;
                    };
                    let Some(cand) = ik.bound.checked_add(kj.bound) else {
                        continue;
                    };
                    let better = match &dist[i][j] {
                        None => true,
                        Some(cur) => cand < cur.bound,
                    };
                    if better {
                        dist[i][j] = Some(Edge {
                            bound: cand,
                            origins: merge_origins(&ik.origins, &kj.origins),
                        });
                    }
                }
            }
        }
        (terms, dist)
    }

    /// `left <= right` が導けるなら、その根拠となった条件のソース位置を返す。
    pub fn proves_le(&self, left: &Affine, right: &Affine) -> Option<Vec<Location>> {
        let needed = right.offset.checked_sub(left.offset)?;
        if left.term == right.term {
            return (0 <= needed).then(Vec::new);
        }
        // 配列の寸法は常に 0 以上（制約辺が空でも 0 <= len a は成り立つ）。
        if left.term == Term::Zero
            && matches!(right.term, Term::Dim(..) | Term::PathDim(..))
            && 0 <= needed
        {
            return Some(Vec::new());
        }
        if self.edges.is_empty() {
            return None;
        }
        let (terms, dist) = self.closure();
        let ui = terms.binary_search(&left.term).ok()?;
        let vi = terms.binary_search(&right.term).ok()?;
        let edge = dist[ui][vi].as_ref()?;
        (edge.bound <= needed).then(|| edge.origins.clone())
    }

    /// `left < right` が導けるなら、その根拠となった条件のソース位置を返す。
    pub fn proves_lt(&self, left: &Affine, right: &Affine) -> Option<Vec<Location>> {
        let shifted = right.clone().shifted(-1)?;
        self.proves_le(left, &shifted)
    }

    /// 特定の条件を満たす項を削除する（削除前に残る項同士の推移関係を保存してから除く）。
    pub fn forget_where(&mut self, mut predicate: impl FnMut(&Term) -> bool) {
        if self.edges.is_empty() {
            return;
        }
        if !self.edges.keys().any(|(u, v)| predicate(u) || predicate(v)) {
            return;
        }
        let (terms, dist) = self.closure();
        let keep: Vec<bool> = terms.iter().map(|t| !predicate(t)).collect();
        let mut next = BTreeMap::new();
        for i in 0..terms.len() {
            if !keep[i] {
                continue;
            }
            for j in 0..terms.len() {
                if i == j || !keep[j] {
                    continue;
                }
                if let Some(edge) = &dist[i][j] {
                    // 暗黙の 0 <= Dim は辺として保持不要。
                    if terms[i] == Term::Zero
                        && matches!(terms[j], Term::Dim(..) | Term::PathDim(..))
                        && edge.bound >= 0
                        && edge.origins.is_empty()
                    {
                        continue;
                    }
                    next.insert((terms[i].clone(), terms[j].clone()), edge.clone());
                }
            }
        }
        self.edges = next;
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
        if paths.iter().any(|f| f.is_empty()) {
            return Facts::default();
        }
        let closures: Vec<_> = paths.iter().map(Facts::closure).collect();
        let (base_terms, base_dist) = &closures[0];
        let mut joined = Facts::default();
        for (i, u) in base_terms.iter().enumerate() {
            for (j, v) in base_terms.iter().enumerate() {
                if i == j {
                    continue;
                }
                let Some(first_edge) = &base_dist[i][j] else {
                    continue;
                };
                if *u == Term::Zero
                    && matches!(v, Term::Dim(..) | Term::PathDim(..))
                    && first_edge.bound >= 0
                    && first_edge.origins.is_empty()
                {
                    continue;
                }
                let mut max_bound = first_edge.bound;
                let mut origins = first_edge.origins.clone();
                let mut all_have = true;
                for (terms, dist) in &closures[1..] {
                    let (Ok(ui), Ok(vi)) = (terms.binary_search(u), terms.binary_search(v)) else {
                        all_have = false;
                        break;
                    };
                    let Some(edge) = &dist[ui][vi] else {
                        all_have = false;
                        break;
                    };
                    if edge.bound > max_bound {
                        max_bound = edge.bound;
                    }
                    origins = merge_origins(&origins, &edge.origins);
                }
                if all_have {
                    joined.add_diff(u.clone(), v.clone(), max_bound, origins);
                }
            }
        }
        joined
    }

    /// クロージャ境界：値キャプチャーで写像できる項だけからなる条件を引き継ぐ。
    pub fn project_captures(&self, mut map_term: impl FnMut(&Term) -> Option<Term>) -> Facts {
        if self.edges.is_empty() {
            return Facts::default();
        }
        let (terms, dist) = self.closure();
        let mapped: Vec<Option<Term>> = terms.iter().map(&mut map_term).collect();
        let mut out = Facts::default();
        for i in 0..terms.len() {
            let Some(u) = &mapped[i] else {
                continue;
            };
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
                if let Some(edge) = &dist[i][j] {
                    if *u == Term::Zero
                        && matches!(v, Term::Dim(..) | Term::PathDim(..))
                        && edge.bound >= 0
                        && edge.origins.is_empty()
                    {
                        continue;
                    }
                    out.add_diff(u.clone(), v.clone(), edge.bound, edge.origins.clone());
                }
            }
        }
        out
    }
}
