use crate::test_util::compile_and_run_c23;

#[test]
fn pointer_types_preserve_nonnil_guarantees_across_calls_fields_and_captures() {
    let output = compile_and_run_c23(
        r#"
struct State { value: i32 }
struct Holder { state: mut State }
fn update(state: mut State) -> void { state#value += 1 }
fn read(holder: Holder) -> i32 { return holder.state.value }
fnshape Read() -> i32
fn invoke(f: fn Read) -> i32 { return f() }
fn identity(value: mut int) -> mut int { return value }
fn clear(slot: mut mut? i32) -> void { #slot = nil }
fn main() -> u8 {
    arena {
        var state = new State { value: 1 }
        update(state)
        state = new State { value: state.value + 1 }
        if read(Holder { state: state }) != 3 { return 1 }
        if invoke(fn [var state]() -> i32 { return state.value }) != 3 { return 2 }
        fix number = new (9 as i16)
        fix same = identity(number)
        if .same != 9 { return 3 }
        fix optional: mut? i32 = new (7 as i32)
        fix slot = new optional
        clear(slot)
        let value = .slot { return 4 }
        fix outer: mut? mut i16 = new number
        let inner = outer { if ..inner != 9 { return 5 } }
    }
    return 0
}
"#,
        "nonnil.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn pointer_branch_join_is_order_independent_and_only_changes_outer_nilability() {
    let output = compile_and_run_c23(
        r#"
struct Node { value: i32, next: mut? Node }
fn read(p: mut Node) -> i32 { return p.value }
fn main() -> u8 {
    arena {
        fix a = new Node { value: 7, next: nil }
        fix all = ifexpr true { a } else { new Node { value: 1, next: nil } }
        if read(all) != 7 { return 1 }
        fix left = ifexpr true { a } else { nil }
        fix right = ifexpr false { nil } else { a }
        fix field = ifexpr false { a.next } else { a }
        fix matched = matchexpr true { true => a
            false => nil }
        fix nested = ifexpr true { a } else { ifexpr true { nil } else { nil } }
        let p = left { if p.value != 7 { return 2 } } else { return 3 }
        let p = right { if p.value != 7 { return 4 } } else { return 5 }
        let p = field { if p.value != 7 { return 6 } } else { return 7 }
        let p = matched { if p.value != 7 { return 8 } } else { return 9 }
        let p = nested { if p.value != 7 { return 10 } } else { return 11 }
        fix literal = ifexpr true { new 42 } else { nil }
        let p = literal { if .p != 42 { return 13 } } else { return 14 }
        fix narrow = new (7 as u8)
        fix inferred = ifexpr true { new 3 } else { narrow }
        if .inferred != 3 { return 15 }
        fix empty: mut? Node = ifexpr true { nil } else { nil }
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
    let prefix = "struct Node { value: i32 }\nfn take(p: mut Node) -> void { return }\nfn clear(p: mut mut? Node) -> void { #p = nil }\n";
    for body in [
        "fix p: mut Node = nil",
        "var p = new Node { value: 1 }\np = nil",
        "take(nil)",
        "fix p: mut? Node = new Node { value: 1 }\ntake(p)",
        "fix p: mut? Node = new Node { value: 1 }\nfix n = p.value",
        "fix p = new Node { value: 1 }\nfix slot = new p\nclear(slot)",
        "fix p = new Node { value: 1 }\nfix slot = new p\nfix alias: mut? mut? Node = slot",
        "fix p = new Node { value: 1 }\nfix q: mut? Node = p\nfix a = new p\nfix b = new q\nfix mixed = ifexpr true { a } else { b }",
        "fix p = ifexpr true { nil } else { nil }",
        "fix p = ifexpr true { new 1 } else { new 2.0 }",
        "fix p = ifexpr true { new 2.0 } else { new 1 }",
        "fix p: mut Node = ifexpr true { nil } else { new Node { value: 1 } }",
        "fix p: mut? Node = new Node { value: 1 }\nobjection [known nonnil] { take(p) }",
    ] {
        let source = format!("{prefix}fn main() -> u8 {{ arena {{ {body} }}\nreturn 0 }}");
        assert!(suz::compile(source.as_bytes()).is_err(), "{body}");
    }
}

#[test]
fn required_initialization_cycles_report_the_fields_and_nilable_edges_break_them() {
    for (types, fields) in [
        ("struct Node { next: mut Node }", vec!["Node.next"]),
        (
            "struct Alpha { b: mut Beta }\nstruct Beta { a: mut Alpha }",
            vec!["Alpha.b", "Beta.a"],
        ),
        (
            "struct Alpha { b: Beta }\nstruct Beta { a: mut Alpha }",
            vec!["Alpha.b", "Beta.a"],
        ),
        (
            "struct Node { next: mut Node, optional: mut? Node }",
            vec!["Node.next"],
        ),
        ("struct Node { next: mut mut Node }", vec!["Node.next"]),
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
        "struct Alpha { b: mut Beta }\nstruct Beta { a: mut? Alpha }",
        "struct Alpha { b: Beta }\nstruct Beta { a: mut? Alpha }",
        "struct Node { next: mut mut? Node }",
        "struct Node { next: mut? mut Node }",
    ] {
        let source = format!("{types}\nfn main() -> u8 {{ return 0 }}");
        suz::compile(source.as_bytes()).unwrap();
    }
}

#[test]
fn objection_proof_uses_member_types_instead_of_initial_values() {
    let source = b"struct Node { value: i32 }\nstruct Holder { p: mut Node }\nfn main() -> u8 { arena { fix outer = new Node { value: 1 }\nfix holder = new Holder { p: outer }\narena { fix inner = new Node { value: 2 }\nfix value = Holder { p: inner }\nobjection [claim] { #holder = value } } }\nreturn 0 }";
    let error = suz::compile(source).unwrap_err();
    assert!(
        error.message().contains("proven lifetime violation"),
        "{error}"
    );
}

#[test]
fn inferred_pointer_depth_limit_reports_an_error_instead_of_overflowing() {
    let mut source = String::from("fn main() -> u8 { arena { fix p0 = new 1\n");
    for i in 1..256 {
        source.push_str(&format!("fix p{i} = new p{}\n", i - 1));
    }
    let valid = format!("{source}}}\nreturn 0 }}");
    suz::check_source(valid.as_bytes(), suz::ArithmeticMode::Default).unwrap();
    source.push_str("fix excessive = new p255\n}\nreturn 0 }");
    let error = suz::check_source(source.as_bytes(), suz::ArithmeticMode::Default).unwrap_err();
    assert!(
        error.message().contains("pointer type nesting exceeds 256"),
        "{error}"
    );
}

#[test]
fn explicit_pointer_depth_uses_the_pointer_limit_diagnostic() {
    for layer in ["mut ", "mut? "] {
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
fn let_only_tests_nil_for_nilable_inputs() {
    // 非nilの引数でもCコンパイラが分岐を取り除くとは限らないため、生成時点で確認する。
    for (ty, expected_checks) in [("mut", 0), ("mut?", 1)] {
        let source = format!(
            "struct Cell {{ value: i32 }}\nfn get(p: {ty} Cell) -> i32 {{ let q = p {{ return q.value }}\nreturn 7 }}\nfn main() -> u8 {{ arena {{ fix p = new Cell {{ value: 3 }}\nreturn get(p) as u8 or 0 }} }}"
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
    let source = b"fn get(p: mut i32) -> i32 { let q = p { return .q } else { return true }\nreturn 7 }\nfn main() -> u8 { return 0 }";
    assert!(suz::compile(source).is_err());
}

#[test]
fn deep_pointer_assignments_cannot_change_inner_nilability() {
    // 62/63段目を内側に持つ型で、型表現の違いが代入の許否を変えないことを確認する。
    // 末端から数えた段を、ソースでは最外側から並べる。
    fn pointer_type(depth: usize, nilable_layer: usize) -> String {
        let mut ty = String::new();
        for layer in (1..=depth).rev() {
            ty.push_str(if layer == nilable_layer {
                "mut? "
            } else {
                "mut "
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
                    "fn take(p: {from}) -> void {{ fix q: {to} = p }}\nfn main() -> u8 {{ return 0 }}"
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

#[test]
fn readonly_pointers_support_shallow_readonly_conversions_joins_generics_and_reject_writes() {
    let output = compile_and_run_c23(
        r#"
struct Inner { x: i32 }
struct Node { value: i32, inner: Inner }
struct Holder { target: mut Node, items: #[i32] }

fn Node.get() -> i32 { return self.value }
fn Node#bump() -> void { self#value += 1 }
fn read_node(p: ref Node) -> i32 { return p.get() + p.inner.x }
fn read_opt(p: ref? Node) -> i32 {
    let q = p { return q.value } else { return -1 }
}
fn generic_read[T](p: ref T) -> T { return .p }

fn main() -> u8 {
    arena {
        fix m = new Node { value: 10, inner: Inner { x: 2 } }
        fix r: ref Node = m
        if read_node(m) != 12 { return 1 }
        if read_node(r) != 12 { return 2 }
        if (.r).value != 10 { return 3 }

        // 暗黙変換: mut -> ref, mut -> ref?, mut? -> ref?, ref -> ref?
        fix ro1: ref? Node = m
        fix mo: mut? Node = m
        fix ro2: ref? Node = mo
        fix ro3: ref? Node = r
        fix ro_nil: ref? Node = nil
        if read_opt(ro1) != 10 || read_opt(ro2) != 10 || read_opt(ro3) != 10 { return 4 }
        if read_opt(ro_nil) != -1 { return 5 }

        // 分岐合流: mut? + ref -> ref?, mut + ref -> ref, ref + nil -> ref?
        fix j1 = ifexpr true { mo } else { r }
        fix j2 = ifexpr false { r } else { mo }
        fix j3 = ifexpr true { m } else { r }
        fix j4 = ifexpr false { r } else { nil }
        let p = j1 { if p.value != 10 { return 6 } } else { return 7 }
        let p = j2 { if p.value != 10 { return 8 } } else { return 9 }
        if j3.value != 10 { return 10 }
        let p = j4 { return 11 }

        // 浅い読み取り専用性: ref経由でも内側のmutポインタ・#[T]への書き込みは許可
        fix items: #[i32] = new #[1, 2]
        fix ref_arr: ref #[i32] = new items
        (.ref_arr)#[0] = 99
        if items.[0] != 99 { return 12 }
        fix holder: ref Holder = new Holder { target: m, items: items }
        holder.target#bump()
        holder.items#[1] = 42
        if m.value != 11 || items.[1] != 42 { return 13 }

        // 8通りの組み合わせ (fix/var × ref/mut × .[T]/#[T])
        fix ro_a: .[i32] = .[1, 2]
        fix ro_b: .[i32] = .[3, 4]
        var v_ref_ro: ref .[i32] = new ro_a
        v_ref_ro = new ro_b
        if ((.v_ref_ro).[0] or 0) != 3 { return 14 }
        fix ro_c: .[i32] = .[5]
        fix f_mut_ro: mut .[i32] = new ro_c
        #f_mut_ro = .[6 as i32]
        if ((.f_mut_ro).[0] or 0) != 6 { return 15 }

        // ジェネリクス推論: mut T を ref T 仮引数へ渡す
        if generic_read(m).value != 11 { return 16 }
        if generic_read(r).value != 11 { return 17 }
    }
    return 0
}
"#,
        "readonly-pointers.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");

    let prefix = r#"
struct Inner { x: i32 }
struct Node { value: i32, inner: Inner }
fn Node#bump() -> void { self#value += 1 }
fn take_mut(p: mut Node) -> void { p#value += 1 }
fn take_mut_opt(p: mut? Node) -> void { let q = p { q#value += 1 } }
"#;
    for (body, rule) in [
        (
            "fix p: ref Node = new Node { value: 1, inner: Inner { x: 2 } }\n#p = Node { value: 3, inner: Inner { x: 4 } }",
            "arena.ptr-operations",
        ),
        (
            "fix p: ref Node = new Node { value: 1, inner: Inner { x: 2 } }\np#value = 3",
            "arena.ptr-operations",
        ),
        (
            "fix p: ref Node = new Node { value: 1, inner: Inner { x: 2 } }\np.inner#x = 3",
            "arena.ptr-operations",
        ),
        (
            "fix p: ref Node = new Node { value: 1, inner: Inner { x: 2 } }\np#bump()",
            "method.write-target",
        ),
        (
            "fix p: ref Node = new Node { value: 1, inner: Inner { x: 2 } }\ntake_mut(p)",
            "type.expected",
        ),
        (
            "fix p: ref? Node = new Node { value: 1, inner: Inner { x: 2 } }\ntake_mut_opt(p)",
            "type.expected",
        ),
        (
            "fix p: ref? Node = new Node { value: 1, inner: Inner { x: 2 } }\nfix n = p.value",
            "arena.nil-deref",
        ),
        (
            "fix a = new Node { value: 1, inner: Inner { x: 2 } }\nfix b = new a\nfix c: mut ref Node = b",
            "type.expected",
        ),
        (
            "fix p: ref #[i32] = new (new #[1 as i32])\n#p = new #[2 as i32]",
            "arena.ptr-operations",
        ),
    ] {
        let source = format!("{prefix}fn main() -> u8 {{ arena {{ {body} }}\nreturn 0 }}");
        let error = suz::check_source(source.as_bytes(), suz::ArithmeticMode::Default).unwrap_err();
        assert_eq!(error.rule(), rule, "{body}: {error}");
    }
}

#[test]
fn null_is_a_regular_identifier_and_diagnostics_use_nil_terms() {
    let output = compile_and_run_c23(
        r#"
struct Holder { null: i32, next: mut? Holder }

fn inc(null: i32) -> i32 {
    return null + 1
}

fn main() -> u8 {
    arena {
        fix null: i32 = 4
        fix h = new Holder { null: null, next: nil }
        fix v = inc(h.null)
        let n = h.next {
            return 1
        }
        if v != 5 { return 2 }
    }
    return 0
}
"#,
        "null-ident.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");

    for (source, rule, expected) in [
        (
            "fn main() -> u8 { fix p = nil\nreturn 0 }",
            "arena.nil",
            "nil requires an expected nilable reference type",
        ),
        (
            "fn main() -> u8 { fix p: mut i32 = nil\nreturn 0 }",
            "arena.nil",
            "nil requires a nilable reference type",
        ),
        (
            "fn main() -> u8 { fix p: mut? i32 = nil\nfix x = .p\nreturn 0 }",
            "arena.nil-deref",
            "use let before dereferencing a possibly nil pointer",
        ),
        (
            "fn main() -> u8 { fix a: .[i32]? = nil\nfix x = a.[0] or 0\nreturn 0 }",
            "array.value",
            "use let before accessing a nilable array",
        ),
        (
            "fn main() -> u8 { fix a: .[i32]? = nil\nfor x in a { }\nreturn 0 }",
            "char.for-type",
            "iteration without a range requires text or a non-nil array",
        ),
        (
            "fix BAD: .[mut i32] = .[]\nfn main() -> u8 { return 0 }",
            "array.constant",
            "constant arrays cannot contain non-nil pointers or non-nil writable arrays",
        ),
        (
            "struct Node { next: mut Node }\nfn main() -> u8 { return 0 }",
            "arena.recursive-struct",
            "non-nil initialization cycle:",
        ),
    ] {
        let error = suz::check_source(source.as_bytes(), suz::ArithmeticMode::Default).unwrap_err();
        assert_eq!(error.rule(), rule, "{source}\n{error}");
        assert!(error.message().contains(expected), "{source}\n{error}");
    }
}
