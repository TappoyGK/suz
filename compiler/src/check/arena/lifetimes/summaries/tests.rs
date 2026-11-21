use super::*;
const HERE: Location = Location { line: 1, column: 1 };

fn body(id: usize) -> Analysis {
    let mut body = Analysis::new(&Scope::default());
    body.function_id = id;
    body
}
fn parameter(body: &mut Analysis, index: usize, ty: Type) -> Node {
    let object = body.cell(0, HERE, ty);
    body.objects.insert(object, Object::Parameter(index, ty));
    let value = body.node();
    body.targets[value].insert(Reference::object(object));
    value
}
fn pointer(body: &mut Analysis, region: usize, ty: Type) -> (Node, Node) {
    let cell = body.cell(region, HERE, ty);
    let value = body.node();
    body.targets[value].insert(Reference::object(cell));
    (value, cell)
}
fn call(id: usize, arguments: Vec<(Type, Node)>, result: Node, region: usize) -> Call {
    Call {
        id,
        arguments: arguments
            .into_iter()
            .map(|(ty, value)| (ty, vec![(vec![], value)]))
            .collect(),
        result: vec![(vec![], result)],
        receiver: None,
        captures: BTreeMap::new(),
        region,
        location: HERE,
        notes: BTreeSet::new(),
        indirect: false,
    }
}
fn identity() -> (BTreeMap<usize, Rc<Analysis>>, BTreeMap<usize, Rc<Summary>>) {
    let mut f = body(1);
    let first = parameter(&mut f, 0, Type::Bool);
    parameter(&mut f, 1, Type::Bool);
    f.returned.insert(vec![], first);
    let bodies = BTreeMap::from([(1, Rc::new(f))]);
    let summaries = infer(&bodies).unwrap();
    (bodies, summaries)
}

#[test]
fn identity_preserves_the_actual_argument_and_caps_only_the_returned_reference() {
    let (_, summaries) = identity();
    assert!(summaries[&1].stores.is_empty());
    assert_eq!(summaries[&1].returned[&vec![]].len(), 1);
    let mut caller = body(0);
    let inner = caller.region(0, HERE);
    let (first, outer) = pointer(&mut caller, 0, Type::Bool);
    let (second, _) = pointer(&mut caller, inner, Type::Bool);
    let result = caller.node();
    caller.direct_calls.push(call(
        1,
        vec![
            (Type::Bool.pointer(), first),
            (Type::Bool.pointer(), second),
        ],
        result,
        inner,
    ));
    let saved = caller.cell(0, HERE, Type::Bool.pointer());
    caller
        .constraints
        .push(Constraint::Store(Place::Direct(saved), result, HERE));
    let mut checked = caller.with_summaries(&summaries).unwrap();
    assert_eq!(
        checked.targets[result],
        BTreeSet::from([Reference {
            target: outer,
            limit: Some(inner)
        }])
    );
    assert_eq!(checked.cells[&outer].region, 0);
    let constraints = checked.constraints.clone();
    assert_eq!(
        checked.validate(&constraints).unwrap_err().rule_id,
        "arena.return-lifetime"
    );
}

#[test]
fn read_only_call_does_not_invent_writes_to_its_argument() {
    let mut f = body(1);
    parameter(&mut f, 0, Type::Bool.pointer());
    let summaries = infer(&BTreeMap::from([(1, Rc::new(f))])).unwrap();
    assert!(summaries[&1].stores.is_empty());
    let mut caller = body(0);
    let inner = caller.region(0, HERE);
    let (argument, outer) = pointer(&mut caller, 0, Type::Bool.pointer());
    let result = caller.node();
    caller.direct_calls.push(call(
        1,
        vec![(Type::Bool.pointer().pointer(), argument)],
        result,
        inner,
    ));
    let mut checked = caller.with_summaries(&summaries).unwrap();
    let constraints = checked.constraints.clone();
    checked.validate(&constraints).unwrap();
    assert!(checked.targets[outer].is_empty());
}

#[test]
fn writes_are_applied_to_the_real_destination_not_the_callers_current_arena() {
    let mut f = body(1);
    let destination = parameter(&mut f, 0, Type::Bool.pointer());
    let source = parameter(&mut f, 1, Type::Bool);
    f.constraints.push(Constraint::Store(
        Place::Indirect(destination, vec![]),
        source,
        HERE,
    ));
    let summaries = infer(&BTreeMap::from([(1, Rc::new(f))])).unwrap();
    let mut caller = body(0);
    let inner = caller.region(0, HERE);
    let (destination, _) = pointer(&mut caller, 0, Type::Bool.pointer());
    let (source, _) = pointer(&mut caller, inner, Type::Bool);
    let result = caller.node();
    caller.direct_calls.push(call(
        1,
        vec![
            (Type::Bool.pointer().pointer(), destination),
            (Type::Bool.pointer(), source),
        ],
        result,
        inner,
    ));
    let mut checked = caller.with_summaries(&summaries).unwrap();
    let constraints = checked.constraints.clone();
    assert_eq!(
        checked.validate(&constraints).unwrap_err().rule_id,
        "arena.store"
    );
}

#[test]
fn allocation_initialization_and_return_origins_cross_the_boundary_together() {
    let mut f = body(1);
    let source = parameter(&mut f, 0, Type::Bool);
    let (returned, allocation) = pointer(&mut f, 0, Type::Bool.pointer());
    f.objects
        .insert(allocation, Object::Allocation(1, allocation));
    f.constraints
        .push(Constraint::Store(Place::Direct(allocation), source, HERE));
    f.returned.insert(vec![], returned);
    let summaries = infer(&BTreeMap::from([(1, Rc::new(f))])).unwrap();
    let mut caller = body(0);
    let inner = caller.region(0, HERE);
    let (source, original) = pointer(&mut caller, 0, Type::Bool);
    let result = caller.node();
    let read = caller.node();
    caller
        .direct_calls
        .push(call(1, vec![(Type::Bool.pointer(), source)], result, inner));
    caller
        .constraints
        .push(Constraint::Load(read, Place::Indirect(result, vec![])));
    let checked = caller.with_summaries(&summaries).unwrap();
    let returned = checked.targets[result].first().unwrap();
    assert_eq!(checked.cells[&returned.target].region, inner);
    assert_eq!(
        checked.targets[read],
        BTreeSet::from([Reference {
            target: original,
            limit: Some(inner)
        }])
    );
}

#[test]
fn mutual_recursion_reaches_a_fixed_point_with_all_return_origins() {
    let mut f = body(1);
    let a = parameter(&mut f, 0, Type::Bool);
    let b = parameter(&mut f, 1, Type::Bool);
    let recursive = f.node();
    f.direct_calls.push(call(
        2,
        vec![(Type::Bool.pointer(), a), (Type::Bool.pointer(), b)],
        recursive,
        0,
    ));
    let result = f.node();
    f.constraints.extend([
        Constraint::Copy(result, a),
        Constraint::Copy(result, recursive),
    ]);
    f.returned.insert(vec![], result);
    let mut g = body(2);
    let a = parameter(&mut g, 0, Type::Bool);
    let b = parameter(&mut g, 1, Type::Bool);
    let result = g.node();
    g.direct_calls.push(call(
        1,
        vec![(Type::Bool.pointer(), b), (Type::Bool.pointer(), a)],
        result,
        0,
    ));
    g.returned.insert(vec![], result);
    let summaries = infer(&BTreeMap::from([(1, Rc::new(f)), (2, Rc::new(g))])).unwrap();
    for id in [1, 2] {
        let origins: BTreeSet<_> = summaries[&id].returned[&vec![]]
            .iter()
            .map(|value| value.object)
            .collect();
        assert_eq!(
            origins,
            BTreeSet::from([
                Object::Parameter(0, Type::Bool),
                Object::Parameter(1, Type::Bool)
            ])
        );
    }
}

#[test]
fn two_calls_do_not_exchange_their_actual_arguments() {
    let (_, summaries) = identity();
    let mut caller = body(0);
    let (a, a_object) = pointer(&mut caller, 0, Type::Bool);
    let (b, b_object) = pointer(&mut caller, 0, Type::Bool);
    let first = caller.node();
    let second = caller.node();
    caller.direct_calls.push(call(
        1,
        vec![(Type::Bool.pointer(), a), (Type::Bool.pointer(), b)],
        first,
        0,
    ));
    caller.direct_calls.push(call(
        1,
        vec![(Type::Bool.pointer(), b), (Type::Bool.pointer(), a)],
        second,
        0,
    ));
    let checked = caller.with_summaries(&summaries).unwrap();
    assert_eq!(
        checked.targets[first],
        BTreeSet::from([Reference {
            target: a_object,
            limit: Some(0)
        }])
    );
    assert_eq!(
        checked.targets[second],
        BTreeSet::from([Reference {
            target: b_object,
            limit: Some(0)
        }])
    );
}

#[test]
fn missing_callee_is_an_error_not_an_empty_effect() {
    let mut caller = body(0);
    let result = caller.node();
    caller.direct_calls.push(call(99, vec![], result, 0));
    let error = match infer(&BTreeMap::from([(0, Rc::new(caller))])) {
        Ok(_) => panic!("unknown callee was accepted"),
        Err(error) => error,
    };
    assert_eq!(error.rule_id, "arena.summary");
}

#[test]
fn callee_objection_is_lexical_and_call_store_keeps_the_nonnull_proof() {
    let mut f = body(1);
    let a = parameter(&mut f, 0, Type::Bool.pointer());
    let b = parameter(&mut f, 1, Type::Bool.pointer());
    let source = parameter(&mut f, 2, Type::Bool);
    let destination = f.node();
    f.constraints.extend([
        Constraint::Copy(destination, a),
        Constraint::Copy(destination, b),
    ]);
    f.objections.insert(f.constraints.len());
    f.nonnull_values.insert(source);
    f.constraints.push(Constraint::Store(
        Place::Indirect(destination, vec![]),
        source,
        HERE,
    ));
    let summaries = infer(&BTreeMap::from([(1, Rc::new(f))])).unwrap();
    for both_older in [false, true] {
        let mut caller = body(0);
        let inner = caller.region(0, HERE);
        let (a, _) = pointer(&mut caller, 0, Type::Bool.pointer());
        let (b, _) = pointer(
            &mut caller,
            if both_older { 0 } else { inner },
            Type::Bool.pointer(),
        );
        let (source, _) = pointer(&mut caller, inner, Type::Bool);
        let result = caller.node();
        let mut invocation = call(
            1,
            vec![
                (Type::Bool.pointer().pointer(), a),
                (Type::Bool.pointer().pointer(), b),
                (Type::Bool.pointer(), source),
            ],
            result,
            inner,
        );
        invocation.location = Location {
            line: 20,
            column: 3,
        };
        caller.direct_calls.push(invocation);
        let mut checked = caller.with_summaries(&summaries).unwrap();
        let constraints = checked.constraints.clone();
        if both_older {
            let error = checked.validate(&constraints).unwrap_err();
            assert_eq!(error.rule_id, "arena.store");
            assert!(error.message.contains("proven lifetime violation"));
            assert!(
                error
                    .related
                    .iter()
                    .any(|note| note.rule_id == "arena.call-diagnostic" && note.line == 20)
            );
        } else {
            // 本体のobjectionは呼び出し側では効かない。混合候補なら未証明の拒否。
            let error = checked.validate(&constraints).unwrap_err();
            assert_eq!(error.rule_id, "arena.store");
            assert!(error.message.contains("may outlive"));
        }
    }
}

#[test]
fn allocation_sites_of_different_specializations_do_not_merge() {
    let mut bodies = BTreeMap::new();
    for id in [10, 11] {
        let mut f = body(id);
        let (returned, object) = pointer(&mut f, 0, Type::Bool);
        f.objects.insert(object, Object::Allocation(id, object));
        f.returned.insert(vec![], returned);
        bodies.insert(id, Rc::new(f));
    }
    let summaries = infer(&bodies).unwrap();
    let a = &summaries[&10].returned[&vec![]];
    let b = &summaries[&11].returned[&vec![]];
    assert!(a.is_disjoint(b));
}

#[test]
fn recursive_allocation_sites_remain_finite() {
    let mut f = body(1);
    let (fresh, object) = pointer(&mut f, 0, Type::Bool);
    f.objects.insert(object, Object::Allocation(1, object));
    let recursive = f.node();
    f.direct_calls.push(call(1, vec![], recursive, 0));
    let returned = f.node();
    f.constraints.extend([
        Constraint::Copy(returned, fresh),
        Constraint::Copy(returned, recursive),
    ]);
    f.returned.insert(vec![], returned);
    let summaries = infer(&BTreeMap::from([(1, Rc::new(f))])).unwrap();
    assert_eq!(summaries[&1].objects.len(), 1);
    assert_eq!(
        summaries[&1].returned[&vec![]]
            .iter()
            .map(|value| value.object)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([Object::RecursiveAllocation(1, Type::Bool)])
    );
}

#[test]
fn repeated_calls_with_the_same_storage_do_not_duplicate_allocation_graphs() {
    let mut f = body(1);
    parameter(&mut f, 0, Type::Bool);
    let (returned, allocated) = pointer(&mut f, 0, Type::Bool);
    f.objects
        .insert(allocated, Object::Allocation(1, allocated));
    f.returned.insert(vec![], returned);
    let summaries = infer(&BTreeMap::from([(1, Rc::new(f))])).unwrap();
    let mut caller = body(0);
    let storage = caller.cell(0, HERE, Type::Bool.pointer());
    for _ in 0..128 {
        let argument = caller.node();
        caller
            .constraints
            .push(Constraint::Load(argument, Place::Direct(storage)));
        let result = caller.node();
        caller
            .direct_calls
            .push(call(1, vec![(Type::Bool.pointer(), argument)], result, 0));
        caller
            .constraints
            .push(Constraint::Store(Place::Direct(storage), result, HERE));
    }
    let checked = caller.with_summaries(&summaries).unwrap();
    assert_eq!(
        checked
            .objects
            .values()
            .filter(|object| **object == Object::Allocation(1, allocated))
            .count(),
        1
    );
    assert_eq!(checked.targets[storage].len(), 1);
}

#[test]
fn mutable_receivers_with_the_same_value_keep_distinct_storage() {
    let mut f = body(1);
    let receiver = f.cell(0, HERE, Type::Bool.pointer());
    f.objects.insert(receiver, Object::Slot(0));
    let source = parameter(&mut f, 1, Type::Bool);
    f.constraints
        .push(Constraint::Store(Place::Direct(receiver), source, HERE));
    let summaries = infer(&BTreeMap::from([(1, Rc::new(f))])).unwrap();
    let mut caller = body(0);
    let inner = caller.region(0, HERE);
    let (source, _) = pointer(&mut caller, inner, Type::Bool);
    let empty = caller.node();
    for region in [inner, 0] {
        let receiver = caller.cell(region, HERE, Type::Bool.pointer());
        let result = caller.node();
        let mut invocation = call(
            1,
            vec![
                (Type::Bool.pointer(), empty),
                (Type::Bool.pointer(), source),
            ],
            result,
            inner,
        );
        invocation.receiver = Some(Place::Direct(receiver));
        caller.direct_calls.push(invocation);
    }
    let mut checked = caller.with_summaries(&summaries).unwrap();
    let constraints = checked.constraints.clone();
    assert_eq!(
        checked.validate(&constraints).unwrap_err().rule_id,
        "arena.store"
    );
}

fn recursive_ring(count: usize, writes_input: bool) -> BTreeMap<usize, Rc<Analysis>> {
    let tokens =
        crate::lexer::lex("struct Node { v: i32, next: ptr? Node }\nfn main() -> u8 { return 0 }")
            .unwrap();
    let program = crate::parser::parse(&tokens).unwrap();
    let structs = Rc::new(crate::check::type_parameters::structs::Structs::new(
        &program,
    ));
    structs.prepare().unwrap();
    let ty = Type::Struct(0);
    let mut bodies = BTreeMap::new();
    for id in 0..count {
        let mut f = body(id);
        f.structs = structs.clone();
        let p = parameter(&mut f, 0, ty);
        let q = parameter(&mut f, 1, ty);
        for argument in [p, q] {
            let object = f.targets[argument].first().unwrap().target;
            let field = f.project(object, &[1]);
            f.targets[field].insert(Reference::object(object));
        }
        let (fresh, object) = pointer(&mut f, 0, ty);
        f.objects.insert(object, Object::Allocation(id, object));
        let field = f.project(object, &[1]);
        f.constraints
            .push(Constraint::Store(Place::Direct(field), q, HERE));
        if writes_input {
            f.constraints
                .push(Constraint::Store(Place::Indirect(p, vec![1]), q, HERE));
        }
        let recursive = f.node();
        let invocation = call(
            (id + 1) % count,
            vec![(ty.pointer(), fresh), (ty.pointer(), p)],
            recursive,
            0,
        );
        f.direct_calls.push(invocation);
        let result = f.node();
        f.constraints.extend([
            Constraint::Copy(result, q),
            Constraint::Copy(result, recursive),
        ]);
        f.returned.insert(vec![], result);
        bodies.insert(id, Rc::new(f));
    }
    bodies
}

#[test]
fn swapping_arguments_around_a_recursive_ring_has_bounded_work_per_function() {
    for count in [10, 20, 40, 80] {
        let inferred = infer_checked(recursive_ring(count, false), true, BTreeMap::new()).unwrap();
        assert!(
            inferred.evaluations <= 12 * count,
            "{} evaluations for {count}",
            inferred.evaluations
        );
        for summary in inferred.summaries.values() {
            assert!(summary.objects.len() <= 3);
            assert!(summary.stores.len() <= 6);
        }
        let candidates: usize = inferred
            .checked
            .values()
            .flat_map(|body| &body.targets)
            .map(BTreeSet::len)
            .sum();
        assert!(
            candidates <= 500 * count,
            "{candidates} candidates for {count}"
        );
    }
}

#[test]
fn recursive_allocation_abstraction_keeps_stores_into_older_arguments_visible() {
    let bodies = recursive_ring(10, true);
    let summaries = infer(&bodies).unwrap();
    let mut caller = body(100);
    caller.structs = bodies[&0].structs.clone();
    let ty = Type::Struct(0);
    let inner = caller.region(0, HERE);
    let (older, _) = pointer(&mut caller, 0, ty);
    let (younger, _) = pointer(&mut caller, inner, ty);
    let result = caller.node();
    let invocation = call(
        0,
        vec![(ty.pointer(), older), (ty.pointer(), younger)],
        result,
        inner,
    );
    caller.direct_calls.push(invocation);
    let mut checked = caller.with_summaries(&summaries).unwrap();
    let constraints = checked.constraints.clone();
    assert_eq!(
        checked.validate(&constraints).unwrap_err().rule_id,
        "arena.store"
    );
}

#[test]
fn forwarding_substitution_matches_constraint_solving_and_excludes_writes() {
    let (_, summaries) = identity();
    let mut f = body(2);
    f.return_region = Some(0);
    let a = parameter(&mut f, 0, Type::Bool);
    let b = parameter(&mut f, 1, Type::Bool);
    let result = f.node();
    f.direct_calls.push(call(
        1,
        vec![(Type::Bool.pointer(), b), (Type::Bool.pointer(), a)],
        result,
        0,
    ));
    f.returned.insert(vec![], result);
    let substituted = f.forwarded_summary(&summaries).unwrap();
    let solved = f.with_summaries(&summaries).unwrap().summary();
    assert_eq!(substituted, solved);
    assert_eq!(
        substituted.returned[&vec![]].first().unwrap().object,
        Object::Parameter(1, Type::Bool)
    );
    f.constraints
        .push(Constraint::Store(Place::Indirect(a, vec![]), b, HERE));
    assert!(f.forwarded_summary(&summaries).is_none());
}

#[test]
fn direct_parameter_returns_need_no_call_graph_expansion() {
    let mut f = body(2);
    f.return_region = Some(0);
    let value = parameter(&mut f, 0, Type::Bool);
    f.returned.insert(vec![], value);
    let summaries = BTreeMap::new();
    assert_eq!(
        f.forwarded_summary(&summaries).unwrap(),
        f.with_summaries(&summaries).unwrap().summary()
    );
    let region = f.region(0, HERE);
    let local = f.cell(region, HERE, Type::Bool);
    f.constraints
        .push(Constraint::Address(value, Place::Direct(local)));
    assert!(f.forwarded_summary(&summaries).is_none());
}

#[test]
fn alternative_targets_share_allocations_but_not_call_sites_or_regions() {
    for count in [16, 64, 256] {
        let mut caller = body(0);
        let mut summaries = BTreeMap::new();
        let mut results = Vec::new();
        for id in 1..=count {
            let mut target = body(id);
            let (value, object) = pointer(&mut target, 0, Type::Bool);
            target
                .objects
                .insert(object, Object::Allocation(id, object));
            target.returned.insert(vec![], value);
            summaries.insert(id, Rc::new(target.summary()));
            let result = caller.node();
            results.push(result);
            let mut invocation = call(id, vec![], result, 0);
            invocation.indirect = true;
            caller.direct_calls.push(invocation);
        }
        let other_site = caller.node();
        let mut invocation = call(1, vec![], other_site, 0);
        invocation.indirect = true;
        invocation.location.line = 2;
        caller.direct_calls.push(invocation);
        let inner = caller.region(0, HERE);
        let other_region = caller.node();
        let mut invocation = call(1, vec![], other_region, inner);
        invocation.indirect = true;
        caller.direct_calls.push(invocation);
        let checked = caller.with_summaries(&summaries).unwrap();
        let first = &checked.targets[results[0]];
        assert_eq!(first.len(), 1);
        for result in results {
            assert_eq!(&checked.targets[result], first);
        }
        assert!(first.is_disjoint(&checked.targets[other_site]));
        assert!(first.is_disjoint(&checked.targets[other_region]));
        // 呼び出し候補が増えても実体候補は増えない。式の位置・Arenaの違いは保持する。
        assert_eq!(checked.cells.len(), 3);
    }
}
