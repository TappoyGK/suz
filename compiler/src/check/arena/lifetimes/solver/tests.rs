use super::*;
use crate::{
    ast::{Location, Type},
    check::scope::Scope,
};
const HERE: Location = Location { line: 1, column: 1 };

// 参照用の単純な全体反復。増分や依存辺を持たず、制約の定義から候補を求める。
fn reference(analysis: &mut Analysis<'_>, constraints: &[Constraint]) {
    loop {
        let before = analysis.targets.clone();
        for constraint in constraints {
            let (destinations, values) = match constraint {
                Constraint::Copy(dst, src) => (vec![*dst], analysis.targets[*src].clone()),
                Constraint::Load(dst, place) => {
                    let mut values = BTreeSet::new();
                    for cell in analysis.destinations(place) {
                        values.extend(&analysis.targets[cell]);
                    }
                    (vec![*dst], values)
                }
                Constraint::Address(dst, place) => (
                    vec![*dst],
                    analysis.destinations(place).into_iter().collect(),
                ),
                Constraint::Store(place, src, _) => {
                    (analysis.destinations(place), analysis.targets[*src].clone())
                }
            };
            for dst in destinations {
                analysis.targets[dst].extend(&values);
            }
        }
        if before == analysis.targets {
            return;
        }
    }
}

// 動的に作るフィールドのノード番号は生成順で変わるので、実体とフィールドの組で比較する。
fn snapshot(analysis: &Analysis<'_>, original_nodes: usize) -> Vec<BTreeSet<(Node, Vec<usize>)>> {
    (0..original_nodes)
        .map(|node| {
            analysis.targets[node]
                .iter()
                .map(|target| {
                    let cell = &analysis.cells[target];
                    (cell.root, cell.path.clone())
                })
                .collect()
        })
        .collect()
}

fn fixture(scope: &Scope, count: usize) -> Analysis<'_> {
    let mut analysis = Analysis::new(scope);
    for _ in 0..count {
        analysis.cell(0, HERE, Type::Void);
    }
    for cell in 0..count {
        let node = analysis.node();
        analysis.targets[node].insert(cell);
    }
    analysis
}

#[test]
fn delta_solver_matches_full_iteration_with_aliases_and_cycles() {
    let scope = Scope::default();
    let count = 8;
    for seed in 1..=64_u64 {
        let mut random = seed;
        let mut pick = |limit| {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((random >> 32) as usize) % limit
        };
        let mut constraints = Vec::new();
        for _ in 0..80 {
            let src = count + pick(count);
            let dst = count + pick(count);
            let cell = pick(count);
            let pointer = count + pick(count);
            let place = if pick(2) == 0 {
                Place::Direct(cell)
            } else {
                Place::Indirect(pointer, vec![pick(2)])
            };
            constraints.push(match pick(4) {
                0 => Constraint::Copy(dst, src),
                1 => Constraint::Load(dst, place),
                2 => Constraint::Store(place, src, HERE),
                // フィールドのアドレスは別の末端ノードへ渡す。任意の型への再解釈はSuzではできない。
                _ => Constraint::Address(2 * count, place),
            });
        }
        let mut actual = fixture(&scope, count);
        let mut expected = fixture(&scope, count);
        actual.node();
        expected.node();
        let originals = actual.targets.len();
        Worklist::default().run(&mut actual, &constraints);
        reference(&mut expected, &constraints);
        assert_eq!(
            snapshot(&actual, originals),
            snapshot(&expected, originals),
            "seed {seed}"
        );
        // 値だけでなく、間接Storeが各実体へ保存した候補も一致することを確かめる。
        for (path, cell) in &actual.projections {
            let other = expected.projections[path];
            let values = |a: &Analysis<'_>, node: Node| -> BTreeSet<_> {
                a.targets[node]
                    .iter()
                    .map(|n| (a.cells[n].root, a.cells[n].path.clone()))
                    .collect()
            };
            assert_eq!(
                values(&actual, *cell),
                values(&expected, other),
                "seed {seed}, {path:?}"
            );
        }
        assert_eq!(actual.projections.len(), expected.projections.len());
    }
}

#[test]
fn reverse_copy_cycle_propagates_each_candidate_without_rescanning_old_sets() {
    let scope = Scope::default();
    for count in [64, 128] {
        let mut analysis = fixture(&scope, count);
        let constraints: Vec<_> = (0..count)
            .map(|i| Constraint::Copy(count + i, count + (i + 1) % count))
            .collect();
        let mut work = Worklist::default();
        work.run(&mut analysis, &constraints);
        for targets in &analysis.targets[count..] {
            assert_eq!(targets.len(), count);
        }
        // 実時間ではなく、コピー先へ渡した候補数を数える。全体反復の三乗への退行を検出する。
        assert!(
            work.propagated <= 3 * count * count,
            "{} for {count}",
            work.propagated
        );
    }
}
