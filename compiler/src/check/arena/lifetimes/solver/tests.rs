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
                let accepted: Vec<_> = values
                    .iter()
                    .copied()
                    .filter(|value| {
                        analysis
                            .call_regions
                            .get(&dst)
                            .is_none_or(|region| analysis.cells[value].region == *region)
                    })
                    .collect();
                analysis.targets[dst].extend(accepted);
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
        // 領域を限定した呼び出し効果も、後から増える候補や循環を含めて全体反復と比べる。
        if seed % 2 == 0 {
            for analysis in [&mut actual, &mut expected] {
                analysis.cells.get_mut(&1).unwrap().region = 1;
                analysis.cells.get_mut(&3).unwrap().region = 1;
                analysis.call_regions.insert(count + 2, 0);
                analysis.call_regions.insert(count + 4, 1);
                analysis.targets[count + 4].clear();
            }
        }
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

#[test]
fn mixed_arena_cycles_do_not_repeat_store_candidates_for_each_read() {
    let scope = Scope::default();
    for count in [32, 64] {
        let mut analysis = fixture(&scope, 2 * count);
        let inner = analysis.region(0, HERE);
        for cell in count..2 * count {
            analysis.cells.get_mut(&cell).unwrap().region = inner;
        }
        let outer_values = 2 * count;
        let inner_values = 3 * count;
        let mut constraints = Vec::new();
        for i in 0..count {
            constraints.push(Constraint::Copy(
                outer_values + i,
                outer_values + (i + 1) % count,
            ));
            constraints.push(Constraint::Copy(
                inner_values + i,
                inner_values + (i + 1) % count,
            ));
        }
        constraints.push(Constraint::Copy(outer_values, inner_values));
        for i in 0..5 * count {
            // if letのスナップショットと、保存元の変数を毎回読み出す形を再現する。
            let snapshot = analysis.cell(inner, HERE, Type::Void);
            let pointer = analysis.node();
            let value = analysis.node();
            constraints.push(Constraint::Store(
                Place::Direct(snapshot),
                outer_values + i % count,
                HERE,
            ));
            constraints.push(Constraint::Load(pointer, Place::Direct(snapshot)));
            constraints.push(Constraint::Copy(value, inner_values + i % count));
            constraints.push(Constraint::Store(
                Place::Indirect(pointer, vec![0]),
                value,
                HERE,
            ));
        }
        let mut work = Worklist::default();
        work.run(&mut analysis, &constraints);
        for cell in 0..2 * count {
            let field = analysis.project(cell, &[0]);
            assert_eq!(analysis.targets[field], (count..2 * count).collect());
        }
        assert!(
            work.propagated <= 30 * count * count,
            "{} for {count}",
            work.propagated
        );
    }
}

#[test]
fn aliases_of_filtered_values_preserve_the_region_filter() {
    let scope = Scope::default();
    let mut actual = fixture(&scope, 2);
    let mut expected = fixture(&scope, 2);
    for analysis in [&mut actual, &mut expected] {
        analysis.cells.get_mut(&1).unwrap().region = 1;
        let filtered = analysis.node();
        analysis.call_regions.insert(filtered, 0);
        analysis.node();
    }
    let constraints = [
        Constraint::Copy(4, 2),
        Constraint::Copy(4, 3),
        Constraint::Copy(5, 4),
        Constraint::Copy(4, 5),
    ];
    Worklist::default().run(&mut actual, &constraints);
    reference(&mut expected, &constraints);
    assert_eq!(actual.targets, expected.targets);
    assert_eq!(actual.targets[5], BTreeSet::from([0]));
}

#[test]
fn connected_arena_calls_do_not_repeat_dense_propagation_per_call() {
    use crate::ast::{StructDeclaration, StructField};
    let ty = Type::Struct(0);
    let scope = Scope {
        structs: std::rc::Rc::new(vec![StructDeclaration {
            name: "Node".into(),
            location: HERE,
            start: 0,
            end: 0,
            fields: ["next", "other"]
                .into_iter()
                .map(|name| StructField {
                    name: name.into(),
                    ty: ty.pointer(),
                    location: HERE,
                })
                .collect(),
        }]),
        ..Scope::default()
    };
    for count in [32, 64] {
        let mut analysis = Analysis::new(&scope);
        let mut arguments = Vec::new();
        for _ in 0..count {
            let cell = analysis.cell(0, HERE, ty);
            let node = analysis.node();
            analysis.targets[node].insert(cell);
            arguments.push((
                ty.pointer(),
                std::collections::BTreeMap::from([(vec![], node)]),
            ));
        }
        // 連結する実体の数と呼び出し数を一緒に増やす。戻り値も実体のフィールドへ保存する。
        for i in 0..5 * count {
            let a = &arguments[i % count];
            let b = &arguments[(i + 1) % count];
            let result = analysis.arena_call(&[a.clone(), b.clone()], ty.pointer(), 0, HERE);
            analysis.constraints.push(Constraint::Store(
                Place::Indirect(a.1[&vec![]], vec![1]),
                result[&vec![]],
                HERE,
            ));
        }
        let constraints = std::mem::take(&mut analysis.constraints);
        let mut work = Worklist::default();
        work.run(&mut analysis, &constraints);
        analysis.validate(&constraints).unwrap();
        // 時間に依存しない作業量の上限。呼び出しごとの密なグラフによる三乗への退行を検出する。
        assert!(
            work.propagated <= 100 * count * count,
            "{} for {count}",
            work.propagated
        );
    }
}
