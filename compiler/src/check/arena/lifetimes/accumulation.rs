//! 実体へ到達する保存場所の字句領域を、一つのグラフで伝播する。
//! ループごとの全探索や、呼び出し経路の配列の複製を行わない。
use super::*;
use std::{cmp::Ordering, collections::VecDeque};

/// 呼び出し先の経路を共有する、診断用の連結リスト。寿命効果には含めない。
#[derive(Clone, Debug)]
pub(super) struct Trace(Rc<TraceStep>);
#[derive(Debug)]
struct TraceStep {
    origin: Location,
    depth: usize,
    site: Location,
    notes: Vec<summaries::CallNote>,
    next: Option<Trace>,
}
impl PartialEq for Trace {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for Trace {}
impl PartialOrd for Trace {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Trace {
    fn cmp(&self, other: &Self) -> Ordering {
        // 要約の共有用。経路の深さに比例する比較を避け、異なる経路は混同しない。
        (self.0.origin, Rc::as_ptr(&self.0)).cmp(&(other.0.origin, Rc::as_ptr(&other.0)))
    }
}
impl Trace {
    pub fn allocation(site: Location) -> Self {
        Self(Rc::new(TraceStep {
            origin: site,
            depth: 1,
            site,
            notes: vec![],
            next: None,
        }))
    }
    pub fn through(&self, call: &summaries::Call) -> Self {
        Self(Rc::new(TraceStep {
            origin: self.0.origin,
            depth: self.0.depth + 1,
            site: call.location,
            notes: call.notes.iter().cloned().collect(),
            next: Some(self.clone()),
        }))
    }
    pub fn origin(&self) -> Location {
        self.0.origin
    }
    fn notes(&self) -> Vec<Diagnostic> {
        let mut notes = Vec::new();
        // 注記の組み立ても表示も、警告数×経路の全長には比例させない。
        const MAX_STEPS: usize = 16;
        let mut shown = 0;
        let mut current = Some(self);
        while let Some(trace) = current {
            let step = &trace.0;
            if shown == MAX_STEPS && step.next.is_some() {
                notes.push(step.site.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "arena.call-diagnostic",
                        "{} intermediate allocation calls omitted",
                        step.depth.saturating_sub(1)
                    ),
                ));
                notes.push(step.origin.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("arena.call-diagnostic", "the object is allocated here"),
                ));
                break;
            }
            shown += 1;
            // 先頭の位置は警告本体に表示済み。関数値の候補はこの後で残す。
            if shown > 1 {
                notes.push(
                    step.site.error(
                        ErrorKind::InvalidSource,
                        crate::DiagnosticMessage {
                            rule: "arena.call-diagnostic",
                            text: if step.next.is_some() {
                                "this call leads to the allocation"
                            } else {
                                "the object is allocated here"
                            }
                            .into(),
                        },
                    ),
                );
            }
            for note in &step.notes {
                notes.push(note.location.error(
                    ErrorKind::InvalidSource,
                    crate::DiagnosticMessage {
                        rule: "arena.call-diagnostic",
                        text: note.message.clone(),
                    },
                ));
            }
            current = step.next.as_ref();
        }
        notes
    }
}

#[derive(Clone)]
pub(super) struct Allocation {
    pub site: Location,
    pub trace: Trace,
    // 確保先Arenaの内側にある最外の反復。この反復の外へ残れば警告しない。
    pub loop_region: Option<usize>,
}
#[derive(Clone)]
pub(super) struct Discarded {
    pub site: Location,
    pub trace: Trace,
    pub region: usize,
    pub loop_region: Option<usize>,
}

struct Escapes {
    // 領域木の前順番号。部分木の範囲内なら、その反復の中だけに保存される。
    starts: Vec<usize>,
    ends: Vec<usize>,
    // この実体へ到達する保存場所の前順番号の最小・最大。未到達の最小値はMAX。
    ranges: Vec<(usize, usize)>,
}
impl Escapes {
    fn from(analysis: &Analysis) -> Self {
        let mut children = vec![Vec::new(); analysis.regions.len()];
        for (id, region) in analysis.regions.iter().enumerate() {
            if let Some(parent) = region.parent {
                children[parent].push(id);
            }
        }
        let mut starts = vec![0; children.len()];
        let mut ends = starts.clone();
        let mut stack = vec![(0, false)];
        let mut clock = 0;
        while let Some((id, visited)) = stack.pop() {
            if visited {
                ends[id] = clock;
                continue;
            }
            starts[id] = clock;
            clock += 1;
            stack.push((id, true));
            stack.extend(children[id].iter().rev().map(|&child| (child, false)));
        }
        let mut graph = vec![Vec::new(); analysis.targets.len()];
        let mut ranges = vec![(usize::MAX, 0); graph.len()];
        // 確保先Arenaの寿命は保持元に数えない。局所の実体へ保存しただけで
        // 反復外へ残ると誤認しないよう、変数・仮引数などからの到達を伝える。
        for (&node, cell) in &analysis.cells {
            graph[cell.root].extend(
                analysis.targets[node]
                    .iter()
                    .map(|r| analysis.cells[&r.target].root),
            );
            if node == cell.root
                && !matches!(
                    analysis.objects.get(&node),
                    Some(
                        summaries::Object::Allocation(..)
                            | summaries::Object::RecursiveAllocation(..)
                    )
                )
            {
                ranges[node] = (starts[cell.region], starts[cell.region]);
            }
        }
        for node in analysis
            .kept
            .iter()
            .chain(analysis.returned.values())
            .flat_map(|&node| &analysis.targets[node])
        {
            ranges[analysis.cells[&node.target].root] = (0, 0);
        }
        // 閉路は一つにまとめ、その後はDAGの各辺を一度だけ通す。
        let representatives = solver::partition(&graph);
        let mut edges = vec![Vec::new(); graph.len()];
        let mut indegrees = vec![0usize; graph.len()];
        for (node, outgoing) in graph.iter().enumerate() {
            let from = representatives[node];
            ranges[from].0 = ranges[from].0.min(ranges[node].0);
            ranges[from].1 = ranges[from].1.max(ranges[node].1);
            for &target in outgoing {
                let to = representatives[target];
                if from != to {
                    edges[from].push(to);
                    indegrees[to] += 1;
                }
            }
        }
        let mut queue: VecDeque<_> = (0..graph.len())
            .filter(|&n| representatives[n] == n && indegrees[n] == 0)
            .collect();
        while let Some(node) = queue.pop_front() {
            for &target in &edges[node] {
                ranges[target].0 = ranges[target].0.min(ranges[node].0);
                ranges[target].1 = ranges[target].1.max(ranges[node].1);
                indegrees[target] -= 1;
                if indegrees[target] == 0 {
                    queue.push_back(target);
                }
            }
        }
        for node in 0..graph.len() {
            ranges[node] = ranges[representatives[node]];
        }
        Self {
            starts,
            ends,
            ranges,
        }
    }
    fn stays_in(&self, node: Node, region: usize) -> bool {
        let (first, last) = self.ranges[node];
        first == usize::MAX || (first >= self.starts[region] && last < self.ends[region])
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
        let previous = self.active_loop;
        self.active_loop = previous.or(Some(iteration));
        if let Some(condition) = condition {
            self.expr(condition, iteration, arena);
        }
        self.statements(body, iteration, arena);
        self.active_loop = previous;
        self.locals = locals;
    }

    pub(super) fn discarded_allocation(&self) -> Option<Trace> {
        let inherited = self
            .discarded_calls
            .iter()
            .filter(|call| call.region == 0)
            .map(|call| call.trace.clone());
        if self.allocations.is_empty() {
            return inherited.min_by_key(Trace::origin);
        }
        let escapes = Escapes::from(self);
        self.allocations
            .iter()
            .filter(|(node, _)| self.cells[node].region == 0 && escapes.ranges[**node].0 != 0)
            .flat_map(|(_, allocations)| allocations.iter().map(|a| a.trace.clone()))
            .chain(inherited)
            .min_by_key(Trace::origin)
    }

    pub(super) fn loop_warnings(&self) -> Vec<(Location, String, Vec<Diagnostic>)> {
        let has_allocations = self
            .allocations
            .values()
            .flatten()
            .any(|a| a.loop_region.is_some());
        let escapes = has_allocations.then(|| Escapes::from(self));
        let mut sites = BTreeMap::new();
        if let Some(escapes) = escapes {
            for (&node, allocations) in &self.allocations {
                for allocation in allocations {
                    if let Some(region) = allocation.loop_region
                        && escapes.stays_in(node, region)
                    {
                        sites
                            .entry(allocation.site)
                            .or_insert((&allocation.trace, self.cells[&node].region));
                    }
                }
            }
        }
        for call in &self.discarded_calls {
            if call.loop_region.is_some() {
                sites.entry(call.site).or_insert((&call.trace, call.region));
            }
        }
        sites.into_iter().map(|(site, (trace, region))| {
            let end = self.regions[region].end;
            let origin = trace.origin();
            (site, format!("allocation does not escape this iteration and accumulates in the current arena (allocation at {}:{}, arena ends at {}:{})", origin.line, origin.column, end.line, end.column), trace.notes())
        }).collect()
    }
}
