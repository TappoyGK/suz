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
fn dimension_relations_follow_aliases_views_and_nonnil_bindings() {
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
    ] {
        let source = format!(
            "fn make() -> #[i64] {{ return new #[0; 3] }}\nfn f(a: #[i64; n], b: #[i64], offset: i64) -> void {{ {body} }}\nfn main() -> u8 {{ return 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "array.access-handler", "{body}: {error}");
    }
    let warn_source = "fn f(a: #[i64; n]) -> void { for i in 0..<n { fix v = a.[i] or 0 } }\nfn main() -> u8 { return 0 }";
    let warnings = suz::check_source(warn_source.as_bytes(), suz::ArithmeticMode::Default).unwrap();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].rule(), "array.redundant-handler");
    assert!(!warnings[0].related.is_empty(), "{warnings:?}");
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

#[test]
fn condition_based_bounds_proofs_work_across_branches_loops_structs_slices_and_closures() {
    let output = compile_and_run_c23(
        r#"
struct Frame { palette: .[i32] }
fnshape Read() -> i32
fn run(f: fn Read) -> i32 { return f() }

fn color(palette: .[i32; _], n: i32) -> i32 {
    if 0 <= n < len palette {
        return palette.[n]
    }
    return 16
}

fn color_guard(palette: .[i32; _], n: i32) -> i32 {
    if not (0 <= n < len palette) {
        return 16
    }
    return palette.[n]
}

fn color_or_guard(palette: .[i32; _], n: i32) -> i32 {
    if n < 0 || n >= len palette {
        return 16
    }
    return palette.[n]
}

fn sum_while(a: .[i32]) -> i32 {
    var i: i64 = 0
    var total: i32 = 0
    while 0 <= i < len a {
        total += a.[i]
        i += 1
    }
    return total
}

fn from_struct(f: Frame, i: i64) -> i32 {
    if 0 <= i < len f.palette {
        return f.palette.[i]
    }
    return 0
}

fn pick_bound_row(rows: .[.[i32]], i: i64, j: i64) -> i32 {
    if 0 <= i < len rows {
        fix row = rows.[i]
        if 0 <= j < len row {
            return row.[j]
        }
    }
    return 0
}

fn join_keeps_common_bound(a: .[i32; n], flag: bool, i: i64) -> i32 {
    if flag {
        if not (0 <= i < n) { return 0 }
    } else {
        if not (0 <= i < n - 1) { return 0 }
    }
    return a.[i]
}

fn conditional_slice(a: .[i32], s: i64, e: i64) -> i64 {
    if 0 <= s <= e <= len a {
        fix sub = a.[s:e]
        return len sub as i64
    }
    return -1
}

fn closure_inherits_value_capture_condition(a: .[i32], i: i64) -> i32 {
    if 0 <= i < len a {
        return run(fn [a, i]() -> i32 { return a.[i] })
    }
    return 0
}

fn match_and_ifexpr(a: .[i32], i: i64) -> i32 {
    fix first = ifexpr 0 <= i < len a { a.[i] } else { 99 }
    if 1 < len a {
        match i {
            0 => { return a.[i] + first }
            1 => { return a.[i] + first }
            _ => { return first }
        }
    }
    return first
}

fn main() -> u8 {
    fix pal: .[i32] = .[10, 20, 30]
    if color(pal, 1) != 20 || color(pal, 5) != 16 { return 1 }
    if color_guard(pal, 2) != 30 || color_guard(pal, -1) != 16 { return 2 }
    if color_or_guard(pal, 0) != 10 || color_or_guard(pal, 3) != 16 { return 3 }
    if sum_while(pal) != 60 { return 4 }
    if from_struct(Frame { palette: pal }, 2) != 30 { return 5 }
    fix rows: .[.[i32]] = .[.[1, 2], .[3]]
    if pick_bound_row(rows, 0, 1) != 2 || pick_bound_row(rows, 1, 0) != 3 { return 6 }
    if join_keeps_common_bound(pal, true, 2) != 30 || join_keeps_common_bound(pal, false, 1) != 20 { return 7 }
    if conditional_slice(pal, 1, 3) != 2 || conditional_slice(pal, 2, 1) != -1 { return 8 }
    if closure_inherits_value_capture_condition(pal, 1) != 20 { return 9 }
    if match_and_ifexpr(pal, 1) != 40 { return 10 }
    return 0
}
"#,
        "condition-bounds-proof.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn condition_bounds_forget_on_writes_calls_methods_pointers_and_joins() {
    for (label, source) in [
        (
            "var index reassigned",
            "fn f(a: .[i32]) -> i32 {\n    var i: i64 = 0\n    if 0 <= i < len a {\n        i = 1\n        return a.[i]\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "var array reassigned",
            "fn f(x: .[i32], y: .[i32], i: i64) -> i32 {\n    var a = x\n    if 0 <= i < len a {\n        a = y\n        return a.[i]\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "while after increment",
            "fn f(a: .[i32]) -> i32 {\n    var i: i64 = 0\n    while 0 <= i < len a {\n        i += 1\n        return a.[i]\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "or single side",
            "fn f(a: .[i32], i: i64, flag: bool) -> i32 {\n    if 0 <= i < len a || flag {\n        return a.[i]\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "ref captured var mutated in condition call",
            "fnshape Step() -> bool\nfn invoke(g: fn Step) -> bool { return g() }\nfn f(a: .[i32]) -> i32 {\n    var i: i64 = 0\n    if 0 <= i < len a && invoke(fn [var i]() -> bool { i = 99\nreturn true }) {\n        return a.[i]\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "var struct field reassigned",
            "struct Frame { palette: .[i32] }\nfn f(x: .[i32], y: .[i32], i: i64) -> i32 {\n    var fr = Frame { palette: x }\n    if 0 <= i < len fr.palette {\n        fr#palette = y\n        return fr.palette.[i]\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "var struct mutable method call",
            "struct Frame { palette: .[i32] }\nfn Frame#swap(other: .[i32]) -> void { self#palette = other }\nfn f(x: .[i32], y: .[i32], i: i64) -> i32 {\n    var fr = Frame { palette: x }\n    if 0 <= i < len fr.palette {\n        fr#swap(y)\n        return fr.palette.[i]\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "loop header forgets var written in body",
            "fn f(a: .[i32]) -> i32 {\n    var i: i64 = 0\n    if 0 <= i < len a {\n        while true {\n            fix v = a.[i]\n            i += 1\n            return v\n        }\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "join drops stronger branch bound",
            "fn f(a: .[i32; n], flag: bool, i: i64) -> i32 {\n    if flag {\n        if not (0 <= i < n) { return 0 }\n    } else {\n        if not (0 <= i < n - 1) { return 0 }\n    }\n    return a.[i + 1]\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "pointer field array not tracked",
            "struct Holder { arr: .[i32] }\nfn f(p: mut Holder, i: i64) -> i32 {\n    if 0 <= i < len p.arr {\n        return p.arr.[i]\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "non-constant array element not tracked",
            "fn f(rows: .[.[i32]], i: i64, j: i64) -> i32 {\n    if 0 <= i < len rows {\n        if 0 <= j < len rows.[i] {\n            return rows.[i].[j]\n        }\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "closure does not inherit ref captured var condition",
            "fnshape Read() -> i32\nfn run(g: fn Read) -> i32 { return g() }\nfn f(a: .[i32]) -> i32 {\n    var i: i64 = 0\n    if 0 <= i < len a {\n        return run(fn [a, var i]() -> i32 { i = 0\nreturn a.[i] })\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
    ] {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "array.access-handler", "{label}: {error}");
    }
}

#[test]
fn redundant_array_handler_warns_at_or_with_related_condition_and_supports_allow() {
    let source = r#"
fn color(palette: .[i32; _], n: i32) -> i32 {
    if 0 <= n < len palette {
        return palette.[n] or 16
    }
    return 16
}
fn main() -> u8 { return 0 }
"#;
    let warnings = suz::check_source(source.as_bytes(), suz::ArithmeticMode::Default).unwrap();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].rule(), "array.redundant-handler");
    assert_eq!(warnings[0].line, 4);
    assert_eq!(warnings[0].column, 28);
    assert!(!warnings[0].related.is_empty(), "{warnings:?}");
    assert_eq!(warnings[0].related[0].line, 3);

    let allowed = r#"
fn color(palette: .[i32; _], n: i32) -> i32 {
    if 0 <= n < len palette {
        return palette.[n] or 16 allow [array.redundant-handler: keep fallback during refactor]
    }
    return 16
}
fn main() -> u8 { return 0 }
"#;
    let warnings = suz::check_source(allowed.as_bytes(), suz::ArithmeticMode::Default).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn post_read_unsigned_and_remainder_bounds_proofs_work_and_empty_rem_panics_as_div_zero() {
    let output = compile_and_run_c23(
        r#"
fn read_twice(a: .[i32], i: i64) -> i32 {
    fix v = a.[i] or return -1
    return v + a.[i]
}

fn sum_len_t(a: .[i32]) -> i32 {
    var i: len_t = 0
    var total: i32 = 0
    while i < len a {
        total += a.[i]
        i += 1
    }
    return total
}

fn pick_u32(palette: .[i32], n: u32) -> i32 {
    return palette.[n % len palette]
}

fn pick_u32_dim(palette: .[i32; m], n: u32) -> i32 {
    fix idx = n % m
    return palette.[idx]
}

fn pick_signed_checked(palette: .[i32], n: i32) -> i32 {
    if 0 <= n {
        return palette.[n % len palette]
    }
    return 0
}

fn main() -> u8 {
    fix pal: .[i32] = .[10, 20, 30]
    if read_twice(pal, 1) != 40 || read_twice(pal, 9) != -1 { return 1 }
    if sum_len_t(pal) != 60 { return 2 }
    if pick_u32(pal, 5) != 30 { return 3 }
    if pick_u32_dim(pal, 4) != 20 { return 4 }
    if pick_signed_checked(pal, 3) != 10 || pick_signed_checked(pal, -1) != 0 { return 5 }
    return 0
}
"#,
        "step2-bounds-proof.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");

    // 符号付き i32 で 0 <= n を確認していない n % len palette は or を要求する。
    let signed_err = suz::compile(
        b"fn f(palette: .[i32], n: i32) -> i32 {\n    return palette.[n % len palette]\n}\nfn main() -> u8 { return 0 }",
    )
    .unwrap_err();
    assert_eq!(signed_err.rule(), "array.access-handler", "{signed_err}");

    // len palette == 0 のときは、配列範囲外ではなく 0 除算でパニックする。
    let div_zero = compile_and_run_c23(
        r#"
fn pick(palette: .[i32], n: u32) -> i32 {
    return palette.[n % len palette]
}
fn main() -> u8 {
    fix empty: .[i32] = .[]
    fix v = pick(empty, 3)
    return v as u8 or 0
}
"#,
        "step2-rem-div-zero.suz",
    );
    assert_eq!(div_zero.status.code(), Some(101), "{div_zero:?}");
    let stderr = String::from_utf8_lossy(&div_zero.stderr);
    assert!(!stderr.contains("[rt.array-bounds]"), "{stderr}");
    assert!(
        stderr.contains("[rt.div-zero]") || stderr.contains("[rt.int-overflow]"),
        "{stderr}"
    );
}

#[test]
fn assignments_inside_expressions_and_conditional_fallbacks_do_not_forge_bounds_proofs() {
    // 指摘 1（F20）: 式の中の代入・複合代入・配列/構造体フィールド再代入の後ろで条件を忘れる。
    for (label, source) in [
        (
            "expr block binding",
            "fn f(a: .[i32]) -> i32 {\n    var i: i64 = 0\n    if 0 <= i < len a {\n        fix z: i32 = expr { i = 77\n1 }\n        return a.[i] + z\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "expr block inside && condition",
            "fn f(a: .[i32]) -> i32 {\n    var i: i64 = 0\n    if 0 <= i < len a && expr { i = 77\ntrue } {\n        return a.[i]\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "ifexpr arm assignment",
            "fn f(a: .[i32], c: i32) -> i32 {\n    var i: i64 = 0\n    if 0 <= i < len a {\n        fix z: i32 = ifexpr c == 0 { expr { i = 77\n1 } } else { 2 }\n        return a.[i] + z\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "matchexpr arm assignment",
            "fn f(a: .[i32], c: i32) -> i32 {\n    var i: i64 = 0\n    if 0 <= i < len a {\n        fix z: i32 = matchexpr c {\n            0 => expr { i = 77\n1 }\n            _ => 2\n        }\n        return a.[i] + z\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "or fallback expr block assignment",
            "fn f(a: .[i32], y: .[i32]) -> i32 {\n    var i: i64 = 0\n    if 0 <= i < len a {\n        fix z: i32 = y.[5] or expr { i = 77\n1 }\n        return a.[i] + z\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "call argument expr block assignment",
            "fn id(x: i32) -> i32 { return x }\nfn f(a: .[i32]) -> i32 {\n    var i: i64 = 0\n    if 0 <= i < len a {\n        fix z: i32 = id(expr { i = 77\n1 })\n        return a.[i] + z\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "binary lhs expr block assignment",
            "fn f(a: .[i32]) -> i32 {\n    var i: i64 = 0\n    if 0 <= i < len a {\n        return expr { i = 77\n1 } + a.[i]\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "expr block compound assignment",
            "fn f(a: .[i32]) -> i32 {\n    var i: i64 = 0\n    if 0 <= i < len a {\n        fix z: i32 = expr { i += 77\n1 }\n        return a.[i] + z\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "logical rhs expr block assignment",
            "fn f(a: .[i32], c: i32) -> i32 {\n    var i: i64 = 0\n    if 0 <= i < len a {\n        fix z = c == 0 && expr { i = 77\ntrue }\n        if z { return a.[i] }\n        return a.[i]\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "var array reassigned in expr block",
            "fn f(a: .[i32], y: .[i32], i: i64) -> i32 {\n    var b = a\n    if 0 <= i < len b {\n        fix z: i32 = expr { b = y\n1 }\n        return b.[i] + z\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "var struct field reassigned in expr block",
            "struct Frame { palette: .[i32] }\nfn f(a: .[i32], y: .[i32], i: i64) -> i32 {\n    var fr = Frame { palette: a }\n    if 0 <= i < len fr.palette {\n        fix z: i32 = expr { fr#palette = y\n1 }\n        return fr.palette.[i] + z\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "slice bound reassigned in expr block",
            "fn f(a: .[i32]) -> i64 {\n    var s: i64 = 0\n    var e: i64 = 1\n    if 0 <= s <= e <= len a {\n        fix z: i32 = expr { e = 77\n1 }\n        fix sub = a.[s:e]\n        return len sub as i64 + z as i64\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "guard exit then expr block assignment",
            "fn f(a: .[i32]) -> i32 {\n    var i: i64 = 0\n    if not (0 <= i < len a) { return 0 }\n    fix z: i32 = expr { i = 77\n1 }\n    return a.[i] + z\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "while body expr block assignment",
            "fn f(a: .[i32]) -> i32 {\n    var i: i64 = 0\n    while 0 <= i < len a {\n        fix z: i32 = expr { i = 77\n1 }\n        return a.[i] + z\n    }\n    return 0\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "post or-return then expr block assignment",
            "fn f(a: .[i32]) -> i32 {\n    var i: i64 = 0\n    fix v = a.[i] or return 0\n    fix z: i32 = expr { i = 77\n1 }\n    return v + a.[i] + z\n}\nfn main() -> u8 { return 0 }",
        ),
        // 指摘 2（F21）: or の代替値内の or panic は、外側のアクセスが成功した場合に評価されない。
        (
            "fallback or panic read does not prove outer read",
            "fn f(a: .[i32], b: .[i32], i: i64) -> i32 {\n    fix v = b.[0] or (a.[i] or panic `oob`)\n    return v + a.[i]\n}\nfn main() -> u8 { return 0 }",
        ),
        (
            "fallback or panic slice does not prove outer slice",
            "fn f(a: .[i32], b: .[i32], s: i64, e: i64) -> i64 {\n    fix v = b.[0:1] or (a.[s:e] or panic `oob`)\n    fix w = a.[s:e]\n    return len v as i64 + len w as i64\n}\nfn main() -> u8 { return 0 }",
        ),
    ] {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "array.access-handler", "{label}: {error}");
    }

    // 書き込みも expr ブロック内代入の後ろでは実行時範囲検査を行いパニックする。
    let write_oob = compile_and_run_c23(
        r#"
fn f(a: #[i32]) -> void {
    var i: i64 = 0
    if 0 <= i < len a {
        fix z: i32 = expr { i = 77
            1 }
        a#[i] = z
    }
}
fn main() -> u8 {
    fix a: #[i32] = new #[10, 20, 30]
    f(a)
    return 0
}
"#,
        "expr-block-write-oob.suz",
    );
    assert_eq!(write_oob.status.code(), Some(101), "{write_oob:?}");
    assert!(
        String::from_utf8_lossy(&write_oob.stderr).contains("[rt.array-bounds]"),
        "{write_oob:?}"
    );

    // 指摘 1（F22）: 内側の for の break が外側の while / loop の後ろの条件を消さない。
    let nested_break = compile_and_run_c23(
        r#"
fn f_while(a: .[i32], c: bool) -> i32 {
    var i: i64 = 5
    while i < 0 || i >= len a {
        for k in 0..<3 { if c { break } }
        i -= 1
    }
    return a.[i]
}
fn f_loop(a: .[i32], c: bool) -> i32 {
    var i: i64 = 5
    loop {
        for k in 0..<3 { if c { break } }
        if 0 <= i < len a { break }
        i -= 1
    }
    return a.[i]
}
fn main() -> u8 {
    fix p: .[i32] = .[10, 20, 30]
    if f_while(p, true) != 30 { return 1 }
    if f_loop(p, true) != 30 { return 2 }
    return 0
}
"#,
        "nested-for-break-outer-loop.suz",
    );
    assert_eq!(nested_break.status.code(), Some(0), "{nested_break:?}");
}
