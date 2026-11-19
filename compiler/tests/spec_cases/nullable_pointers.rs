use crate::test_util::compile_and_run_c23;

#[test]
fn pointer_types_preserve_nonnull_guarantees_across_calls_fields_and_captures() {
    let output = compile_and_run_c23(
        r#"
struct State { value: i32 }
struct Holder { state: ptr State }
fn update(state: ptr State) -> void { state.value += 1 }
fn read(holder: Holder) -> i32 { return holder.state.value }
fnshape Read() -> i32
fn invoke(f: fn Read) -> i32 { return f() }
arena fn identity(value: ptr int) -> ptr int { return value }
arena fn clear(slot: ptr ptr? i32) -> void { *slot = null }
fn main() -> u8 {
    arena {
        var ptr state = new State { value: 1 }
        update(state)
        state = new State { value: state.value + 1 }
        if read(Holder { state: state }) != 3 { return 1 }
        if invoke(fn [var state]() -> i32 { return state.value }) != 3 { return 2 }
        fix ptr number = new (9 as i16)
        fix ptr same = identity(number)
        if *same != 9 { return 3 }
        fix ptr optional: ptr? i32 = new (7 as i32)
        fix ptr slot = new optional
        clear(slot)
        let value = *slot { return 4 }
        fix ptr outer: ptr? ptr i16 = new number
        let inner = outer { if **inner != 9 { return 5 } }
    }
    return 0
}
"#,
        "nonnull.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn pointer_branch_join_is_order_independent_and_only_changes_outer_nullability() {
    let output = compile_and_run_c23(
        r#"
struct Node { value: i32, next: ptr? Node }
fn read(p: ptr Node) -> i32 { return p.value }
fn main() -> u8 {
    arena {
        fix ptr a = new Node { value: 7, next: null }
        fix ptr all = ifexpr true { a } else { new Node { value: 1, next: null } }
        if read(all) != 7 { return 1 }
        fix ptr left = ifexpr true { a } else { null }
        fix ptr right = ifexpr false { null } else { a }
        fix ptr field = ifexpr false { a.next } else { a }
        fix ptr matched = matchexpr true { true => a
            false => null }
        fix ptr nested = ifexpr true { a } else { ifexpr true { null } else { null } }
        let p = left { if p.value != 7 { return 2 } } else { return 3 }
        let p = right { if p.value != 7 { return 4 } } else { return 5 }
        let p = field { if p.value != 7 { return 6 } } else { return 7 }
        let p = matched { if p.value != 7 { return 8 } } else { return 9 }
        let p = nested { if p.value != 7 { return 10 } } else { return 11 }
        fix ptr literal = ifexpr true { new 42 } else { null }
        let p = literal { if *p != 42 { return 13 } } else { return 14 }
        fix ptr narrow = new (7 as u8)
        fix ptr inferred = ifexpr true { new 3 } else { narrow }
        if *inferred != 3 { return 15 }
        fix ptr empty: ptr? Node = ifexpr true { null } else { null }
        let p = empty { return 12 }
    }
    return 0
}
"#,
        "pointer-joins.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn pointer_type_errors_cannot_be_bypassed_by_aliases_or_objection() {
    let prefix = "struct Node { value: i32 }\nfn take(p: ptr Node) -> void { return }\narena fn clear(p: ptr ptr? Node) -> void { *p = null }\n";
    for body in [
        "fix ptr p: ptr Node = null",
        "var ptr p = new Node { value: 1 }\np = null",
        "take(null)",
        "fix ptr p: ptr? Node = new Node { value: 1 }\ntake(p)",
        "fix ptr p: ptr? Node = new Node { value: 1 }\nfix n = p.value",
        "fix ptr p = new Node { value: 1 }\nfix ptr slot = new p\nclear(slot)",
        "fix ptr p = new Node { value: 1 }\nfix ptr slot = new p\nfix ptr alias: ptr? ptr? Node = slot",
        "fix ptr p = new Node { value: 1 }\nfix ptr q: ptr? Node = p\nfix ptr a = new p\nfix ptr b = new q\nfix ptr mixed = ifexpr true { a } else { b }",
        "fix ptr p = ifexpr true { null } else { null }",
        "fix ptr p = ifexpr true { new 1 } else { new 2.0 }",
        "fix ptr p = ifexpr true { new 2.0 } else { new 1 }",
        "fix ptr p: ptr Node = ifexpr true { null } else { new Node { value: 1 } }",
        "fix ptr p: ptr? Node = new Node { value: 1 }\nobjection [known nonnull] { take(p) }",
    ] {
        let source = format!("{prefix}fn main() -> u8 {{ arena {{ {body} }}\nreturn 0 }}");
        assert!(suz::compile(source.as_bytes()).is_err(), "{body}");
    }
}

#[test]
fn required_initialization_cycles_report_the_fields_and_nullable_edges_break_them() {
    for (types, fields) in [
        ("struct Node { next: ptr Node }", vec!["Node.next"]),
        (
            "struct Alpha { b: ptr Beta }\nstruct Beta { a: ptr Alpha }",
            vec!["Alpha.b", "Beta.a"],
        ),
        (
            "struct Alpha { b: Beta }\nstruct Beta { a: ptr Alpha }",
            vec!["Alpha.b", "Beta.a"],
        ),
        (
            "struct Node { next: ptr Node, optional: ptr? Node }",
            vec!["Node.next"],
        ),
        ("struct Node { next: ptr ptr Node }", vec!["Node.next"]),
    ] {
        let source = format!("{types}\nfn main() -> u8 {{ return 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.message().contains("[arena.recursive-struct]"),
            "{error}"
        );
        for field in fields {
            assert!(error.message().contains(field), "{error}");
        }
    }
    for types in [
        "struct Alpha { b: ptr Beta }\nstruct Beta { a: ptr? Alpha }",
        "struct Alpha { b: Beta }\nstruct Beta { a: ptr? Alpha }",
        "struct Node { next: ptr ptr? Node }",
        "struct Node { next: ptr? ptr Node }",
    ] {
        let source = format!("{types}\nfn main() -> u8 {{ return 0 }}");
        suz::compile(source.as_bytes()).unwrap();
    }
}

#[test]
fn objection_proof_uses_member_types_instead_of_initial_values() {
    let source = b"struct Node { value: i32 }\nstruct Holder { p: ptr Node }\nfn main() -> u8 { arena { fix ptr outer = new Node { value: 1 }\nfix ptr holder = new Holder { p: outer }\narena { fix ptr inner = new Node { value: 2 }\nfix value = Holder { p: inner }\nobjection [claim] { *holder = value } } }\nreturn 0 }";
    let error = suz::compile(source).unwrap_err();
    assert!(
        error.message().contains("proven lifetime violation"),
        "{error}"
    );
}

#[test]
fn inferred_pointer_depth_limit_reports_an_error_instead_of_overflowing() {
    let mut source = String::from("fn main() -> u8 { arena { fix ptr p0 = new 1\n");
    for i in 1..256 {
        source.push_str(&format!("fix ptr p{i} = new p{}\n", i - 1));
    }
    let valid = format!("{source}}}\nreturn 0 }}");
    suz::check_source(valid.as_bytes(), suz::ArithmeticMode::Default).unwrap();
    source.push_str("fix ptr excessive = new p255\n}\nreturn 0 }");
    let error = suz::check_source(source.as_bytes(), suz::ArithmeticMode::Default).unwrap_err();
    assert!(
        error.message().contains("pointer type nesting exceeds 256"),
        "{error}"
    );
}

#[test]
fn explicit_pointer_depth_uses_the_pointer_limit_diagnostic() {
    for layer in ["ptr ", "ptr? "] {
        for depth in [256, 257] {
            let source = format!(
                "struct Other {{ value: {}i32 }}\nfn main() -> u8 {{ return 0 }}",
                layer.repeat(depth)
            );
            let result = suz::check_source(source.as_bytes(), suz::ArithmeticMode::Default);
            if depth == 256 {
                result.unwrap();
            } else {
                let error = result.unwrap_err();
                assert!(error.message().contains("[arena.ptr-type]"), "{error}");
            }
        }
    }
}

#[test]
fn let_only_tests_null_for_nullable_inputs() {
    // 非nullの引数でもCコンパイラが分岐を取り除くとは限らないため、生成時点で確認する。
    for (ty, expected_checks) in [("ptr", 0), ("ptr?", 1)] {
        let source = format!(
            "struct Cell {{ value: i32 }}\nfn get(p: {ty} Cell) -> i32 {{ let q = p {{ return q.value }}\nreturn 7 }}\nfn main() -> u8 {{ arena {{ fix ptr p = new Cell {{ value: 3 }}\nreturn get(p) as u8 or 0 }} }}"
        );
        let c = suz::compile(source.as_bytes()).unwrap();
        // ランタイム内の確保失敗検査ではなく、letが生成する局所変数の検査を数える。
        let checks = c
            .lines()
            .filter(|line| line.contains("suz_local_") && line.contains("!= NULL"))
            .count();
        assert_eq!(checks, expected_checks, "{c}");
    }
    // 実行されないelseも通常どおり型検査する。
    let source = b"fn get(p: ptr i32) -> i32 { let q = p { return *q } else { return true }\nreturn 7 }\nfn main() -> u8 { return 0 }";
    assert!(suz::compile(source).is_err());
}

#[test]
fn deep_pointer_assignments_cannot_change_inner_nullability() {
    // 62/63段目を内側に持つ型で、型表現の違いが代入の許否を変えないことを確認する。
    // 末端から数えた段を、ソースでは最外側から並べる。
    fn pointer_type(depth: usize, nullable_layer: usize) -> String {
        let mut ty = String::new();
        for layer in (1..=depth).rev() {
            ty.push_str(if layer == nullable_layer {
                "ptr? "
            } else {
                "ptr "
            });
        }
        ty.push_str("i32");
        ty
    }
    for depth in [63, 64, 128, 256] {
        let mut layers = vec![0, 62, 63, depth];
        layers.dedup();
        for &from_layer in &layers {
            for &to_layer in &layers {
                let from = pointer_type(depth, from_layer);
                let to = pointer_type(depth, to_layer);
                let source = format!(
                    "fn take(p: {from}) -> void {{ fix ptr q: {to} = p }}\nfn main() -> u8 {{ return 0 }}"
                );
                let result = suz::check_source(source.as_bytes(), suz::ArithmeticMode::Default);
                if from_layer == to_layer || (from_layer == 0 && to_layer == depth) {
                    result.unwrap();
                } else {
                    let error = result.unwrap_err();
                    assert!(
                        error.message().contains("[type.expected]"),
                        "depth={depth}, from={from_layer}, to={to_layer}: {error}"
                    );
                }
            }
        }
    }
}
