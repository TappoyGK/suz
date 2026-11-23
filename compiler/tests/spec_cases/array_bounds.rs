//! 寸法名・範囲保証の適用範囲と、呼び出し形式によらない入口検査を確かめる。
use crate::test_util::compile_and_run_c23;

#[test]
fn proven_float_reads_preserve_order_around_writes_and_calls() {
    // 読み取り同士は直接計算できるが、右辺が配列を書き換える場合は左辺の値を先に保存する。
    let output = compile_and_run_c23(
        r#"
fn change(a: #[f32]) -> f32 { a#[0] = 8.0
    return 2.0 }
fn inspect(a: #[f32; n]) -> f32 {
    var s: f32 = 0.0
    for i in 0..<n { s = a.[i] + change(a) }
    return s
}
fn main() -> u8 {
    fix a = new #[1.0 as f32]
    if inspect(a) != 3.0 || a.[0] != 8.0 { return 1 }
    var x: f32 = 1.0
    fix v = x + expr { x = 5.0
        2.0 }
    if v != 3.0 || x != 5.0 { return 2 }
    return 0
}
"#,
        "array-float-order.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn dimensions_are_not_type_arguments_and_only_bind_at_allowed_declarations() {
    for source in [
        "fn f() -> #[i32; n] { return new #[] }",
        "struct Sample { a: #[i32; n] }",
        "fnshape Shape(#[i32; n]) -> void",
        "fn f(a: #[i32; n]?) -> void {}",
        "fn f(a: #[#[i32; n]]) -> void {}",
        "fn f(a: mut #[i32; n]) -> void {}",
        "fn f() -> void { var a: #[i32; n] = new #[]\na = new #[] }",
    ] {
        let error = suz::compile(format!("{source}\nfn main() -> u8 {{ return 0 }}").as_bytes())
            .unwrap_err();
        assert_eq!(error.rule(), "array.dimension-bind", "{source}: {error}");
    }
    for source in [
        "fn f(a: #[i32; a]) -> void {}",
        "fn f(a: #[i32; n], n: i32) -> void {}",
        "fn f() -> void { fix a: #[i32; n, n] = new #[0; 2, 2] }",
        "fn f() -> void { fix n = 3\nfix a: #[i32; n] = new #[0; 3] }",
    ] {
        assert!(
            suz::compile(format!("{source}\nfn main() -> u8 {{ return 0 }}").as_bytes()).is_err(),
            "{source}"
        );
    }
}

#[test]
fn dimension_entry_guards_apply_to_function_values_closures_and_generic_instances() {
    for call in [
        "same(a, b)",
        "fix f: static fn Same = fn same\nf(a, b)",
        "invoke(fn same, a, b)",
        "invoke(fn [](a: #[i64; n], b: #[i64; n]) -> void { panic `BODY` }, a, b)",
        "generic(a, b)",
    ] {
        let source = format!(
            r#"
fnshape Same(#[i64], #[i64]) -> void
fn same(a: #[i64; n], b: #[i64; n]) -> void {{ panic `BODY` }}
fn generic[T](a: #[T; n], b: #[T; n]) -> void {{ panic `BODY` }}
fn invoke(f: fn Same, a: #[i64], b: #[i64]) -> void {{ f(a, b) }}
fn main() -> u8 {{
    fix a = new #[0; 2]
    fix b = new #[0; 3]
    {call}
    return 0
}}
"#
        );
        let output = compile_and_run_c23(&source, "dimension-guards.suz");
        assert_eq!(output.status.code(), Some(101), "{call}: {output:?}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("n expected=2 actual=3") && error.contains("[rt.array-dimensions]"),
            "{call}: {error}"
        );
        assert!(!error.contains("BODY"), "{call}: {error}");
    }
}

#[test]
fn dimension_relations_follow_aliases_views_and_nonnull_bindings() {
    let output = compile_and_run_c23(
        r#"
fn inspect(a: #[i64; h, w], b: #[i64; w, h]) -> void {
    fix alias = a
    for y in 0..<h { for x in 0..<w {
        b.[x]#[y] = alias.[y].[x]
    } }
    for row in a { for x in 0..<w { row#[x] += row.[x] } }
    fix full = a.[:]
    rev y in 1..<h { rev x in 0..<w - 1 { full.[y - 1]#[x + 1] += full.[y].[x] } }
}
fn optional(a: #[i64]?) -> i64 {
    let value = a {
        fix view: #[i64; n] = value
        var result: i64 = 0
        for i in 0..<n { result += view.[i] }
        return result
    }
    return 0
}
fn main() -> u8 {
    fix a = new #[1; 2, 3]
    fix b = new #[0; 3, 2]
    inspect(a, b)
    if b.[2].[1] != 1 || a.[0].[1] != 4 { return 1 }
    fix row = a.[1]
    if optional(row) != 6 { return 2 }
    fix empty: #[i64] = new #[]
    if optional(empty) != 0 { return 3 }
    return 0
}
"#,
        "dimension-origins.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn bounds_proofs_do_not_use_equal_values_or_unlisted_algebra() {
    for body in [
        "for i in 0..<len a { fix v = b.[i] }",
        "for i in 0..<n { fix v = a.[i * 1] }",
        "for i in 0..<n { fix v = a.[0 + i] }",
        "for i in 0..<n { fix v = a.[i + offset] }",
        "for i in 0..<n { fix v = a.[i + 1] }",
        "for i in 0..<n { fix v = a.[i - 1] }",
        "for i in 0..<n { fix v = make().[i] }",
        "for i in 0..<n { fix v = a.[i] or 0 }",
    ] {
        let source = format!(
            "fn make() -> #[i64] {{ return new #[0; 3] }}\nfn f(a: #[i64; n], b: #[i64], offset: i64) -> void {{ {body} }}\nfn main() -> u8 {{ return 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "array.access-handler", "{body}: {error}");
    }
    let source = "fn main() -> u8 { fix a = new #[0; 3]\nfix b = new #[0; 3]\nfor i in 0..<len a { fix v = b.[i] }\nreturn 0 }";
    assert_eq!(
        suz::compile(source.as_bytes()).unwrap_err().rule(),
        "array.access-handler"
    );
}

#[test]
fn array_bounds_follow_value_captures_but_not_mutable_reference_captures() {
    let output = compile_and_run_c23(
        r#"
fnshape Read() -> i64
fn run(f: fn Read) -> i64 { return f() }
fn sum(a: #[i64; n]) -> i64 {
    return run(fn [a, n]() -> i64 {
        var total: i64 = 0
        for i in 0..<n { total += run(fn [a, i]() -> i64 { return a.[i] }) }
        return total
    })
}
fn main() -> u8 {
    fix a = new #[1, 2, 3]
    if sum(a) != 6 { return 1 }
    return 0
}
"#,
        "array-captured-bounds.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn array_assignment_rejects_overlapping_rhs_paths_without_equating_aliases() {
    let source = "fn main() -> u8 { fix a = new #[0; 2]\na#[0] = expr { a#[1] = 3\n4 }\nreturn 0 }";
    assert_eq!(
        suz::compile(source.as_bytes()).unwrap_err().rule(),
        "assign.rhs-write"
    );
}

#[test]
fn dimension_guards_accept_empty_arrays_and_repeated_axes() {
    let output = compile_and_run_c23(
        r#"
fn square(a: #[i64; n, n]) -> len_t { return n }
fn paired(a: #[i64; n], b: #[i64; n]) -> len_t { return n }
fn main() -> u8 {
    fix a: #[i64] = new #[]
    if paired(a, a) != 0 { return 1 }
    fix g = new #[0; 0, 0]
    if square(g) != 0 { return 2 }
    fix h = new #[0; 3, 3]
    if square(h) != 3 { return 3 }
    return 0
}
"#,
        "empty-dimensions.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn bounds_from_finished_scopes_do_not_apply_to_value_block_locals() {
    let source = r#"
fn f(a: #[i64; n]) -> void {
    for i in 0..<n { fix v = a.[i] }
    fix result = expr { fix i = 100
        a.[i] }
}
fn main() -> u8 { return 0 }
"#;
    assert_eq!(
        suz::compile(source.as_bytes()).unwrap_err().rule(),
        "array.access-handler"
    );
}

#[test]
fn empty_array_iteration_does_not_hide_bound_arithmetic_failure() {
    let prefix = "fn safe(a: #[i64; n]) -> void { for i in 1..<n { fix v = a.[i] } }\nfn underflow(a: #[i64; n]) -> void { for i in 0..<n - 1 { fix v = a.[i + 1] } }\n";
    for (call, expected) in [("safe(a)", 0), ("underflow(a)", 101)] {
        let source =
            format!("{prefix}fn main() -> u8 {{ fix a: #[i64] = new #[]\n{call}\nreturn 0 }}");
        let output = compile_and_run_c23(&source, "empty-array-bounds.suz");
        assert_eq!(output.status.code(), Some(expected), "{output:?}");
        if expected != 0 {
            assert!(String::from_utf8_lossy(&output.stderr).contains("[rt.int-overflow]"));
        }
    }
}
