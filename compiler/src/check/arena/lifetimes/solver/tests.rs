use super::*;
use crate::{
    ast::{Location, Type},
    check::scope::Scope,
};
const HERE: Location = Location { line: 1, column: 1 };

fn limited(analysis: &Analysis, value: Reference, limit: Option<usize>) -> BTreeSet<Reference> {
    let mut result = BTreeSet::from([value]);
    if let Some(limit) = limit
        && value
            .limit
            .is_none_or(|previous| !analysis.outlives(limit, previous))
    {
        if value
            .limit
            .is_none_or(|previous| analysis.outlives(previous, limit))
        {
            result.clear();
        }
        result.insert(Reference {
            limit: Some(limit),
            ..value
        });
    }
    result
}

fn places(analysis: &mut Analysis, place: &Place) -> Vec<Reference> {
    match place {
        Place::Direct(cell) => vec![Reference::object(*cell)],
        Place::Indirect(pointer, path) => analysis.targets[*pointer]
            .clone()
            .into_iter()
            .map(|reference| Reference {
                target: analysis.project(reference.target, path),
                ..reference
            })
            .collect(),
    }
}

// 参照用の単純な全体反復。増分や依存辺を持たず、制約の定義から候補を求める。
fn reference(analysis: &mut Analysis, constraints: &[Constraint]) {
    loop {
        let before = analysis.targets.clone();
        for constraint in constraints {
            let (destinations, values) = match constraint {
                Constraint::Copy(dst, src) => (vec![*dst], analysis.targets[*src].clone()),
                Constraint::Limit(dst, src, limit) => (
                    vec![*dst],
                    analysis.targets[*src]
                        .iter()
                        .flat_map(|&value| limited(analysis, value, Some(*limit)))
                        .collect(),
                ),
                Constraint::Load(dst, place) => {
                    let mut values = BTreeSet::new();
                    for reference in places(analysis, place) {
                        for &value in &analysis.targets[reference.target] {
                            values.extend(limited(analysis, value, reference.limit));
                        }
                    }
                    (vec![*dst], values)
                }
                Constraint::Address(dst, place) => {
                    (vec![*dst], places(analysis, place).into_iter().collect())
                }
                Constraint::Store(place, src, _) => {
                    (analysis.destinations(place), analysis.targets[*src].clone())
                }
            };
            for dst in destinations {
                analysis.targets[dst].extend(values.iter().copied());
            }
        }
        if before == analysis.targets {
            return;
        }
    }
}

// 動的に作るフィールドのノード番号は生成順で変わるので、実体とフィールドの組で比較する。
type ReferenceSnapshot = (Node, Vec<usize>, Option<usize>);

fn snapshot(analysis: &Analysis, original_nodes: usize) -> Vec<BTreeSet<ReferenceSnapshot>> {
    (0..original_nodes)
        .map(|node| {
            analysis.targets[node]
                .iter()
                .map(|target| {
                    let cell = &analysis.cells[&target.target];
                    (cell.root, cell.path.clone(), target.limit)
                })
                .collect()
        })
        .collect()
}

fn fixture(scope: &Scope, count: usize) -> Analysis {
    let mut analysis = Analysis::new(scope);
    analysis.region(0, HERE);
    for _ in 0..count {
        analysis.cell(0, HERE, Type::Void);
    }
    for cell in 0..count {
        let node = analysis.node();
        analysis.targets[node].insert(Reference::object(cell));
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
            constraints.push(match pick(5) {
                0 => Constraint::Copy(dst, src),
                1 => Constraint::Load(dst, place),
                2 => Constraint::Store(place, src, HERE),
                3 => Constraint::Limit(dst, src, 1),
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
            let values = |a: &Analysis, node: Node| -> BTreeSet<_> {
                a.targets[node]
                    .iter()
                    .map(|n| {
                        (
                            a.cells[&n.target].root,
                            a.cells[&n.target].path.clone(),
                            n.limit,
                        )
                    })
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
            // letのスナップショットと、保存元の変数を毎回読み出す形を再現する。
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
            assert_eq!(
                analysis.targets[field],
                (count..2 * count).map(Reference::object).collect()
            );
        }
        assert!(
            work.propagated <= 30 * count * count,
            "{} for {count}",
            work.propagated
        );
    }
}

// 内側の期間に制限した戻り値でも、参照先は外側の実体のままである。
// 参照の持ち出しと参照先への書き込みを別々に拒否し、値のコピーでは制限を失わない。
#[test]
fn limited_reference_keeps_its_object_and_rejects_both_escape_directions() {
    let scope = Scope::default();
    for write_through in [false, true] {
        let mut analysis = Analysis::new(&scope);
        let inner = analysis.region(0, HERE);
        let outer_object = analysis.cell(0, HERE, Type::Void);
        let inner_object = analysis.cell(inner, HERE, Type::Void);
        let original = analysis.node();
        analysis.targets[original].insert(Reference::object(outer_object));
        let returned = analysis.node();
        let alias = analysis.node();
        analysis
            .constraints
            .push(Constraint::Limit(returned, original, inner));
        analysis.constraints.push(Constraint::Copy(alias, returned));
        let outer_variable = analysis.cell(0, HERE, Type::Void);
        let new_value = analysis.node();
        analysis.targets[new_value].insert(Reference::object(inner_object));
        analysis.constraints.push(if write_through {
            Constraint::Store(Place::Indirect(alias, vec![]), new_value, HERE)
        } else {
            Constraint::Store(Place::Direct(outer_variable), alias, HERE)
        });
        let error = analysis.solve().unwrap_err();
        let rule = if write_through {
            "[arena.store]"
        } else {
            "[arena.return-lifetime]"
        };
        assert!(error.to_string().contains(rule), "{error}");
        assert_eq!(
            analysis.targets[alias],
            BTreeSet::from([Reference {
                target: outer_object,
                limit: Some(inner),
            }])
        );
        assert_eq!(analysis.cells[&outer_object].region, 0);
    }
}

#[test]
fn limited_reference_can_be_used_and_stored_inside_its_period() {
    let scope = Scope::default();
    let mut analysis = Analysis::new(&scope);
    let inner = analysis.region(0, HERE);
    let outer_object = analysis.cell(0, HERE, Type::Void);
    let original = analysis.node();
    analysis.targets[original].insert(Reference::object(outer_object));
    let returned = analysis.node();
    let local = analysis.cell(inner, HERE, Type::Void);
    analysis
        .constraints
        .push(Constraint::Limit(returned, original, inner));
    analysis
        .constraints
        .push(Constraint::Store(Place::Direct(local), returned, HERE));
    analysis.solve().unwrap();
    assert_eq!(analysis.targets[local], analysis.targets[returned]);
}

#[test]
fn nested_limits_survive_cycles_and_do_not_lengthen_each_other() {
    let scope = Scope::default();
    let mut analysis = Analysis::new(&scope);
    let outer = analysis.cell(0, HERE, Type::Void);
    let inner = analysis.region(0, HERE);
    let deeper = analysis.region(inner, HERE);
    let source = analysis.node();
    analysis.targets[source].insert(Reference::object(outer));
    let first = analysis.node();
    let second = analysis.node();
    let copied = analysis.node();
    let constraints = [
        Constraint::Limit(first, source, deeper),
        Constraint::Limit(second, first, inner),
        Constraint::Copy(copied, second),
        Constraint::Copy(second, copied),
    ];
    Worklist::default().run(&mut analysis, &constraints);
    assert_eq!(
        analysis.targets[copied],
        BTreeSet::from([Reference {
            target: outer,
            limit: Some(deeper),
        }])
    );
}

#[test]
fn indirect_field_reads_and_addresses_preserve_the_period() {
    let scope = Scope::default();
    for address in [false, true] {
        let mut analysis = Analysis::new(&scope);
        let outer = analysis.cell(0, HERE, Type::Void);
        let field = analysis.project(outer, &[0]);
        let pointee = analysis.cell(0, HERE, Type::Void);
        let inner = analysis.region(0, HERE);
        let source = analysis.node();
        analysis.targets[source].insert(Reference::object(outer));
        let returned = analysis.node();
        let read = analysis.node();
        let initializer = analysis.node();
        analysis.targets[initializer].insert(Reference::object(pointee));
        // 読み出しの登録後に、フィールドの内容が伝わる順序でも制限が維持される。
        let constraints = [
            Constraint::Limit(returned, source, inner),
            if address {
                Constraint::Address(read, Place::Indirect(returned, vec![0]))
            } else {
                Constraint::Load(read, Place::Indirect(returned, vec![0]))
            },
            Constraint::Store(Place::Direct(field), initializer, HERE),
        ];
        Worklist::default().run(&mut analysis, &constraints);
        assert_eq!(
            analysis.targets[read],
            BTreeSet::from([Reference {
                target: if address { field } else { pointee },
                limit: Some(inner),
            }])
        );
    }
}

#[test]
fn period_survives_loading_an_older_reference_from_a_callers_arena_object() {
    let scope = Scope::default();
    let mut analysis = Analysis::new(&scope);
    let inner = analysis.region(0, HERE);
    let container = analysis.cell(inner, HERE, Type::Void);
    let field = analysis.project(container, &[0]);
    let older = analysis.cell(0, HERE, Type::Void);
    analysis.targets[field].insert(Reference::object(older));
    let source = analysis.node();
    analysis.targets[source].insert(Reference::object(container));
    let returned = analysis.node();
    let read = analysis.node();
    let saved = analysis.cell(0, HERE, Type::Void);
    analysis.constraints.extend([
        Constraint::Limit(returned, source, inner),
        Constraint::Load(read, Place::Indirect(returned, vec![0])),
        Constraint::Store(Place::Direct(saved), read, HERE),
    ]);
    let error = analysis.solve().unwrap_err();
    assert!(
        error.to_string().contains("[arena.return-lifetime]"),
        "{error}"
    );
    assert_eq!(analysis.cells[&older].region, 0);
}

#[test]
fn limiting_null_does_not_create_a_reference() {
    let scope = Scope::default();
    let mut analysis = Analysis::new(&scope);
    let inner = analysis.region(0, HERE);
    let null = analysis.node();
    let returned = analysis.node();
    Worklist::default().run(&mut analysis, &[Constraint::Limit(returned, null, inner)]);
    assert!(analysis.targets[returned].is_empty());
}

#[test]
fn objection_does_not_extend_a_returned_references_period() {
    let scope = Scope::default();
    let mut analysis = Analysis::new(&scope);
    let outer = analysis.cell(0, HERE, Type::Void);
    let inner = analysis.region(0, HERE);
    let source = analysis.node();
    analysis.targets[source].insert(Reference::object(outer));
    let returned = analysis.node();
    analysis
        .constraints
        .push(Constraint::Limit(returned, source, inner));
    analysis.objections.insert(analysis.constraints.len());
    analysis
        .constraints
        .push(Constraint::Store(Place::Direct(outer), returned, HERE));
    let error = analysis.solve().unwrap_err();
    assert!(
        error.to_string().contains("[arena.return-lifetime]"),
        "{error}"
    );
}

#[test]
fn root_references_do_not_outlive_an_independent_destination_region() {
    let mut analysis = Analysis::new(&Scope::default());
    let root = analysis.cell(0, HERE, Type::Bool);
    let source = analysis.node();
    analysis.targets[source].insert(Reference::object(root));
    analysis
        .regions
        .push(crate::check::arena::lifetimes::Region {
            parent: None,
            end: HERE,
        });
    let destination = analysis.cell(1, HERE, Type::Bool.pointer());
    analysis
        .constraints
        .push(Constraint::Store(Place::Direct(destination), source, HERE));
    assert_eq!(analysis.solve().unwrap_err().rule_id, "arena.store");
}
