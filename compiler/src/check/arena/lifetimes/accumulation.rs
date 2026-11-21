//! 解決済みの参照グラフから、反復の外へ残らない確保だけを警告する。
//! 警告のために寿命制約や実行時の回収時点を変更しない。
use super::*;

#[derive(Clone)]
pub(super) struct Allocation {
    pub site: Location,
    pub origin: Location,
    pub loops: BTreeSet<usize>,
}
#[derive(Clone)]
pub(super) struct Discarded {
    pub site: Location,
    pub origin: Location,
    pub region: usize,
    pub loops: BTreeSet<usize>,
}

pub(super) struct Reachability(BTreeMap<Node, BTreeSet<Node>>);
impl Reachability {
    pub fn from(analysis: &Analysis) -> Self {
        let mut edges: BTreeMap<Node, BTreeSet<Node>> = BTreeMap::new();
        for (&node, cell) in &analysis.cells {
            edges.entry(cell.root).or_default().extend(
                analysis.targets[node]
                    .iter()
                    .map(|reference| analysis.cells[&reference.target].root),
            );
        }
        Self(edges)
    }
    pub fn reachable(&self, seeds: impl IntoIterator<Item = Node>) -> BTreeSet<Node> {
        let mut seen = BTreeSet::new();
        let mut pending: Vec<_> = seeds.into_iter().collect();
        while let Some(node) = pending.pop() {
            if seen.insert(node)
                && let Some(next) = self.0.get(&node)
            {
                pending.extend(next);
            }
        }
        seen
    }
}

impl Analysis {
    pub(super) fn loop_body(
        &mut self,
        body: &[CheckedStatement],
        condition: Option<&CheckedExpression>,
        region: usize,
        arena: Option<usize>,
    ) {
        let locals = self.locals.clone();
        let iteration = self.region(region, self.regions[region].end);
        let id = self.loops.len();
        self.loops.push(iteration);
        self.active_loops.insert(id);
        if let Some(condition) = condition {
            self.expr(condition, iteration, arena);
        }
        self.statements(body, iteration, arena);
        self.active_loops.remove(&id);
        self.locals = locals;
    }

    fn kept_roots(&self) -> impl Iterator<Item = Node> + '_ {
        self.kept
            .iter()
            .flat_map(|&node| &self.targets[node])
            .map(|reference| self.cells[&reference.target].root)
    }

    // 関数から返すか、引数・捕捉・Cへ残す確保と、内部だけで使い終わる確保を分ける。
    // 内部の一時確保は呼び出し側の戻り値が残っても警告対象から消さない。
    pub(super) fn discarded_allocation(&self) -> Option<Location> {
        let inherited = self
            .discarded_calls
            .iter()
            .filter(|call| call.region == 0)
            .map(|call| call.origin)
            .min();
        if self.allocations.is_empty() {
            return inherited;
        }
        let seeds = self
            .objects
            .iter()
            .filter_map(|(&node, object)| {
                matches!(
                    object,
                    summaries::Object::Parameter(..)
                        | summaries::Object::ReachableParameter(..)
                        | summaries::Object::Slot(..)
                )
                .then_some(node)
            })
            .chain(self.returned.values().copied())
            .chain(self.kept_roots());
        let escaped = Reachability::from(self).reachable(seeds);
        self.allocations
            .iter()
            .filter(|(node, _)| self.cells[node].region == 0 && !escaped.contains(node))
            .map(|(_, allocation)| allocation.origin)
            .chain(inherited)
            .min()
    }

    pub(super) fn loop_warnings(&self) -> Vec<(Location, String)> {
        if self.loops.is_empty() {
            return Vec::new();
        }
        let graph = Reachability::from(self);
        let mut sites = BTreeMap::new();
        for (id, &iteration) in self.loops.iter().enumerate() {
            let seeds = self
                .cells
                .iter()
                .filter_map(|(&node, cell)| {
                    (node == cell.root
                        && !self.outlives(iteration, cell.region)
                        && !self
                            .allocations
                            .get(&node)
                            .is_some_and(|a| a.loops.contains(&id)))
                    .then_some(node)
                })
                .chain(self.kept_roots());
            let escaped = graph.reachable(seeds);
            for (&node, allocation) in &self.allocations {
                let region = self.cells[&node].region;
                if allocation.loops.contains(&id)
                    && self.outlives(region, iteration)
                    && !escaped.contains(&node)
                {
                    sites
                        .entry(allocation.site)
                        .or_insert((allocation.origin, region));
                }
            }
            for call in &self.discarded_calls {
                if call.loops.contains(&id) && self.outlives(call.region, iteration) {
                    sites.entry(call.site).or_insert((call.origin, call.region));
                }
            }
        }
        sites.into_iter().map(|(site, (origin, region))| {
            let end = self.regions[region].end;
            (site, format!("allocation does not escape this iteration and accumulates in the current arena (allocation at {}:{}, arena ends at {}:{})", origin.line, origin.column, end.line, end.column))
        }).collect()
    }
}
