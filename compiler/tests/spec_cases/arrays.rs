//! 配列の実体、境界検査の評価順、領域をまたぐ保存を実行結果と拒否例で検査する。
use crate::test_util::compile_and_run_c23;

#[test]
fn arrays_allocate_read_write_and_keep_empty_distinct_from_nil() {
    let output = compile_and_run_c23(
        r#"
fn read(a: #[i64], i: i64) -> i64 { return a.[i] or -1 }
fn cell(a: #[i64; _, _], y: i64, x: i64) -> i64 { return a.[y].[x] or -1 }
fn main() -> u8 {
    fix a = new #[2, 4, 6]
    a#[1] += 3
    if read(a, 1) != 7 { return 1 }
    if read(a, -1) != -1 { return 2 }
    fix grid = new #[[1, 2], [3, 4]]
    grid.[1]#[0] = 8
    if cell(grid, 1, 0) != 8 { return 3 }
    fix empty: #[i64]? = new #[]
    let p = empty { if len p != 0 { return 4 } } else { return 5 }
    fix absent: #[i64]? = nil
    let p = absent { return 6 }
    fix repeated = new #[9; 3]
    if read(repeated, 2) != 9 { return 7 }
    return 0
}
"#,
        "arrays.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn array_reference_lifetimes_follow_elements_aliases_and_function_calls() {
    let prefix = r#"
struct Node { v: i64 }
fn save(a: #[mut? Node], p: mut? Node) -> void { a#[0] = p }
fn read(a: #[mut? Node]) -> mut? Node { fix p = a.[0] or return nil
return p }
fn wrap(p: mut? Node) -> #[mut? Node] { return new #[p] }
"#;
    for body in [
        "a#[0] = new Node { v: 1 }",
        "fix alias = a\nalias#[0] = new Node { v: 1 }",
        "save(a, new Node { v: 1 })",
    ] {
        let source = format!(
            "{prefix}\nfn main() -> u8 {{ fix a: #[mut? Node] = new #[nil; 2]\narena {{ {body} }}\nreturn 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "arena.store", "{body}: {error}");
    }
    let source = format!(
        "{prefix}\nfn main() -> u8 {{ fix p = new Node {{ v: 7 }}\nfix a = wrap(p)\nlet v = read(a) {{ if v.v != 7 {{ return 1 }} }} else {{ return 2 }}\nreturn 0 }}"
    );
    let output = compile_and_run_c23(&source, "array-lifetimes.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn array_reads_short_circuit_indices_and_lower_return_handlers() {
    let output = compile_and_run_c23(
        r#"
struct Counter { v: i64 }
fn tick(c: mut Counter, n: i64) -> i64 { c#v += 1
    return n }
fn read(a: #[i64; _, _], c: mut Counter, y: i64) -> i64 {
    return a.[tick(c, y)].[tick(c, 0)] or tick(c, -1)
}
fn early(a: #[i64], i: i64) -> i64 {
    fix v = a.[i] or return 42
    return v
}
fn main() -> u8 {
    fix c = new Counter { v: 0 }
    fix a = new #[5; 2, 3]
    if read(a, c, -1) != -1 { return 1 }
    if c.v != 2 { return 2 }
    c#v = 0
    if read(a, c, 0) != 5 { return 3 }
    if c.v != 2 { return 4 }
    fix b = new #[1]
    if early(b, 9) != 42 { return 5 }
    return 0
}
"#,
        "array-order.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn array_elements_support_structs_generics_and_function_values() {
    let output = compile_and_run_c23(
        r#"
struct Box[T] { v: T }
fnshape Read(#[i64], i64) -> i64
fn read[T](a: #[T], i: i64, otherwise: T) -> T { return a.[i] or otherwise }
@needs T: len() -> len_t
fn count[T](a: T) -> len_t { return len a }
fn reader(a: #[i64], i: i64) -> i64 { return a.[i] or -1 }
fn main() -> u8 {
    fix a = new #[Box[i32] { v: 2 }; 3]
    a.[1]#v = 7
    fix b = read(a, 1, Box[i32] { v: 0 })
    if b.v != 7 { return 1 }
    if count(a) != 3 { return 2 }
    fix fs: #[static fn Read] = new #[fn reader]
    fix f = fs.[0]
    fix numbers = new #[8]
    if f(numbers, 0) != 8 { return 3 }
    return 0
}
"#,
        "array-types.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn array_types_initializers_and_recursive_dependencies_are_checked() {
    for (declarations, body, rule) in [
        ("", "fix a = new #[]", "array.initializer-type"),
        ("", "fix a: #[i32; _, _] = new #[]", "array.type"),
        ("", "fix a = new #[[1, 2], [3]]", "array.enumeration"),
        ("", "fix a = new #[[], []]", "array.enumeration"),
        ("", "fix a = new #[0; -1]", "array.layout"),
        ("", "fix a = new #[0; 2147483648]", "array.layout"),
        (
            "",
            "fix a = new #[0; 0, 2147483647, 2147483647, 2147483647]",
            "array.layout",
        ),
        (
            "",
            "fix a = new #[0; 2147483647, 2147483647]",
            "array.layout",
        ),
        (
            "",
            "fix small: i32 = 1\nfix a = new #[small, 2]",
            "array.initializer-type",
        ),
        (
            "",
            "fix a: #[i32] = new #[1]\nfix b: #[i64] = a",
            "type.expected",
        ),
        ("", "fix a: #[i32]? = nil\nfix n = len a", "len.targets"),
        (
            "struct Node { next: #[Node; _, _] }",
            "",
            "arena.recursive-struct",
        ),
    ] {
        let source = format!("{declarations}\nfn main() -> u8 {{ {body}\nreturn 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), rule, "{body}: {error}");
    }
    suz::compile(b"struct Node { next: #[Node] }\nfn main() -> u8 { fix root = Node { next: new #[] }\nreturn 0 }").unwrap();
}

#[test]
fn array_failures_precede_later_indices_and_assignment_values() {
    let cases = [
        (
            "a.[-1:1].[rhs()]#[rhs()] = rhs()",
            "start=-1 end=1 length=2",
        ),
        ("a.[2:1].[rhs()]#[rhs()] = rhs()", "start=2 end=1 length=2"),
        (
            "a.[-1].[:]#[rhs()] = rhs()",
            "dimension=1 index=-1 length=2",
        ),
        ("a.[0]#[3] = rhs()", "dimension=2 index=3 length=3"),
        ("a.[-1]#[rhs()] = rhs()", "dimension=1 index=-1 length=2"),
        (
            "fix huge: u64 = 18446744073709551615\na.[0]#[huge] = rhs()",
            "dimension=2 index=18446744073709551615 length=3",
        ),
    ];
    let sources = cases.iter().map(|(body, _)| {
        format!(
            "fn rhs() -> i64 {{ panic \"RHS evaluated\" }}\nfn main() -> u8 {{ fix a = new #[0; 2, 3]\n{body}\nreturn 0 }}"
        )
    }).collect::<Vec<_>>();
    let inputs = sources
        .iter()
        .map(|source| (source.as_str(), "array-bounds.suz"))
        .collect::<Vec<_>>();
    let outputs = crate::test_util::compile_and_run_c23_batch(&inputs);
    for ((body, reason), output) in cases.iter().zip(outputs) {
        assert!(!output.status.success(), "case {body}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(reason), "{body}: {stderr}");
        assert!(!stderr.contains("RHS evaluated"), "{body}: {stderr}");
    }
}

#[test]
fn array_initializers_evaluate_once_before_dimensions_even_when_empty() {
    let output = compile_and_run_c23(
        r#"
struct Counter { v: i64 }
fn tick(p: mut Counter, v: i64) -> i64 { p#v = p.v * 10 + v
    return v }
fn main() -> u8 {
    fix p = new Counter { v: 0 }
    fix a = new #[tick(p, 7); tick(p, 2), tick(p, 0)]
    if p.v != 720 { return 1 }
    if len a != 2 { return 2 }
    p#v = 0
    fix b = new #[[tick(p, 1), tick(p, 2)], [tick(p, 3), tick(p, 4)]]
    if p.v != 1234 { return 3 }
    return 0
}
"#,
        "array-init-order.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn runtime_dimensions_and_sizes_fail_before_allocating() {
    let cases = [
        ("a", "-1, 0", "dimension 1"),
        ("a", "2147483648, 0", "dimension 1"),
        ("a, b", "2147483647, 2147483647", "size_t"),
        ("0, a, b, a", "2147483647, 2147483647", "size_t"),
    ];
    let sources = cases.iter().map(|(dimensions, arguments, _)| {
        format!(
            "fn allocate(a: i64, b: i64) -> void {{ fix p = new #[0; {dimensions}] }}\nfn main() -> u8 {{ allocate({arguments})\nreturn 0 }}"
        )
    }).collect::<Vec<_>>();
    let inputs = sources
        .iter()
        .map(|source| (source.as_str(), "array-size.suz"))
        .collect::<Vec<_>>();
    let outputs = crate::test_util::compile_and_run_c23_batch(&inputs);
    for ((dimensions, arguments, reason), output) in cases.iter().zip(outputs) {
        assert!(
            !output.status.success(),
            "case {dimensions} ({arguments}): {output:?}"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("rt.array-size") && stderr.contains(reason),
            "{dimensions} ({arguments}): {stderr}"
        );
    }
}

#[test]
fn writes_keep_the_original_element_and_read_the_old_value_before_rhs() {
    let output = compile_and_run_c23(
        r#"
struct Holder { a: #[i64] }
fn replace(h: mut Holder, replacement: #[i64]) -> i64 {
    h#a = replacement
    return 3
}
fn overwrite(a: #[i64]) -> i64 { a#[0] = 20
    return 5 }
fn read(a: #[i64], i: i64) -> i64 { return a.[i] or -1 }
fn main() -> u8 {
    fix a = new #[10]
    fix b = new #[100]
    fix h = new Holder { a: a }
    h.a#[0] += replace(h, b)
    if read(a, 0) != 13 { return 1 }
    if read(h.a, 0) != 100 { return 2 }
    a#[0] += overwrite(a)
    if read(a, 0) != 18 { return 3 }
    return 0
}
"#,
        "array-write-order.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn references_from_return_handlers_and_stored_function_values_keep_lifetime_checks() {
    for callback in [false, true] {
        let reader = if callback {
            "fix fs: #[static fn Read] = new #[fn get]\nfix f = fs.[0]\nfix p = f(a)"
        } else {
            "fix p = get(a)"
        };
        let source = format!(
            r#"
struct Node {{ v: i64 }}
struct Holder {{ p: mut? Node }}
fnshape Read(#[mut? Node]) -> mut? Node
fn get(a: #[mut? Node]) -> mut? Node {{
    fix p = a.[0] or return nil
    return p
}}
fn main() -> u8 {{
    fix h = new Holder {{ p: nil }}
    arena {{
        fix a: #[mut? Node] = new #[new Node {{ v: 1 }}]
        {reader}
        h#p = p
    }}
    return 0
}}
"#
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "arena.store", "{error}");
    }
}

#[test]
fn arrays_of_references_use_each_inner_array_length_and_share_its_storage() {
    let output = compile_and_run_c23(
        r#"
fn read(a: #[#[i64]], y: i64, x: i64) -> i64 { return a.[y].[x] or -1 }
fn main() -> u8 {
    fix short = new #[1]
    fix long = new #[2, 3]
    fix rows = new #[short, long]
    rows.[1]#[1] = 7
    if read(rows, 1, 1) != 7 { return 1 }
    if read(rows, 0, 1) != -1 { return 2 }
    if read(rows, -1, 0) != -1 { return 3 }
    return 0
}
"#,
        "arrays-of-references.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn array_element_fields_reject_shorter_lived_references() {
    let source = b"struct Node { v: i32 }\nstruct Item { p: mut? Node, n: i32 }\nfn main() -> u8 {\n    arena { fix a = new #[Item { p: nil, n: 0 }; 3]\n      arena { fix t = new Node { v: 2 }\n        a.[0]#p = t } }\n    return 0 }";
    let error = suz::compile(source).unwrap_err();
    assert_eq!(error.rule(), "arena.store", "{error}");
}

#[test]
fn array_source_diagnostics_are_not_internal_errors() {
    use std::{fs, process::Command};
    let directory = crate::test_util::TemporaryDirectory::new();
    for (source, rule) in [
        (
            "fn f() -> #[i32; n] { return new #[] }\nfn main() -> u8 { return 0 }",
            "array.dimension-bind",
        ),
        (
            "fn main() -> u8 { fix a = new #[0; 3]\na.[0] = 1\nreturn 0 }",
            "array.write",
        ),
    ] {
        let path = directory.0.join("source.suz");
        fs::write(&path, source).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_suz"))
            .arg("check")
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let message = String::from_utf8_lossy(&output.stderr);
        assert!(message.contains(&format!("[{rule}]")), "{message}");
        if rule == "array.write" {
            assert!(message.contains("a#[i]"), "{message}");
        } else {
            assert!(message.contains("dimension names require"), "{message}");
        }
    }
}

#[test]
fn array_zero_initialization_preserves_negative_zero_and_nonzero_values() {
    let output = compile_and_run_c23(
        r#"
fn get(a: #[f64], i: i64) -> f64 { return a.[i] or 10.0 }
fn main() -> u8 {
    fix positive = new #[0.0; 4]
    fix negative = new #[-0.0; 4]
    fix filled = new #[1.5; 4]
    if get(positive, 3) != 0.0 { return 1 }
    if get(filled, 3) != 1.5 { return 2 }
    fix x = 1.0 / get(negative, 3)
    if x >= 0.0 { return 3 }
    return 0
}
"#,
        "array-zero.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn array_access_snapshots_mutable_references_across_index_calls_and_blocks() {
    let output = compile_and_run_c23(
        r#"
fnshape Index() -> i64
fn invoke(f: fn Index) -> i64 { return f() }
fn read(a: #[i64], i: i64) -> i64 { return a.[i] or -1 }
fn main() -> u8 {
    fix first = new #[10]
    fix second = new #[20]
    var current = first
    fix old = current.[invoke(fn [var current, second]() -> i64 {
        current = second
        return 0
    })] or -1
    if old != 10 || read(current, 0) != 20 { return 1 }
    current = first
    fix again = current.[expr { current = second
        0 }] or -1
    if again != 10 { return 2 }
    fix captured = invoke(fn [first]() -> i64 { return first.[0] })
    fix blocked = expr { fix n = read(second, 0)
        first.[0] + n }
    if captured != 10 || blocked != 30 { return 3 }
    fix grid = new #[[10], [20]]
    var row: i64 = 0
    fix original = grid.[row].[expr { row = 1
        0 }] or -1
    if original != 10 { return 4 }
    return 0
}
"#,
        "array-reference-snapshots.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn array_views_share_storage_and_compose_with_reads_and_writes() {
    let output = compile_and_run_c23(
        r#"
fn read(a: #[i64; _, _], y: i64, x: i64) -> i64 { return a.[y].[x] or -1 }
fn row(a: #[i64; _, _], y: i64) -> #[i64] {
    fix r = a.[y] or return new #[]
    return r
}
fn main() -> u8 {
    fix grid = new #[[1, 2, 3], [4, 5, 6]]
    fix r = row(grid, 1)
    r#[1] = 9
    if read(grid, 1, 1) != 9 { return 1 }
    fix view = grid.[1:2]
    view.[0]#[2] = 8
    if read(grid, 1, 2) != 8 { return 2 }
    fix full = grid.[:].[:]
    full.[0]#[0] = 7
    grid.[0:1].[0]#[1] = 10
    if read(grid, 0, 0) != 7 || read(grid, 0, 1) != 10 { return 3 }
    fix empty = grid.[2:2]
    if len empty != 0 { return 4 }
    fix missing = row(grid, 9)
    if len missing != 0 { return 5 }
    fix v = (grid.[0:1]).[0].[2]
    if v != 3 { return 6 }
    return 0
}
"#,
        "array-views.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn array_view_failure_skips_later_brackets_but_evaluates_both_slice_bounds() {
    let output = compile_and_run_c23(
        r#"
struct Counter { n: i64 }
fn tick(c: mut Counter, v: i64) -> i64 { c#n += 1
return v }
fn main() -> u8 {
    fix a = new #[7; 2, 3]
    fix c = new Counter { n: 0 }
    fix v = a.[tick(c, -1):tick(c, 2)].[tick(c, 0)].[tick(c, 0)] or tick(c, 99)
    if v != 99 || c.n != 3 { return 1 }
    c#n = 0
    fix w = a.[tick(c, 8)].[tick(c, 0):tick(c, 1)].[tick(c, 0)] or tick(c, 88)
    if w != 88 || c.n != 2 { return 2 }
    fix r = (a.[9] or new #[4; 3]).[1:2] or panic "slice"
    fix x = r.[0]
    if x != 4 { return 3 }
    return 0
}
"#,
        "array-view-order.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn array_iteration_reads_current_elements_and_handles_empty_rows() {
    let output = compile_and_run_c23(
        r#"
struct Node { n: i64 }
struct Holder { a: #[i64], calls: i64 }
fn source(h: mut Holder) -> #[i64] { h#calls += 1
return h.a }
fn main() -> u8 {
    fix original = new #[1, 2, 3]
    fix h = new Holder { a: original, calls: 0 }
    var sum: i64 = 0
    for value in source(h) {
        sum += value
        original#[2] = 9
        h#a = new #[99]
    }
    if sum != 12 || h.calls != 1 { return 1 }
    var order: i64 = 0
    rev value in original {
        if value == 2 { continue }
        order = order * 10 + value
    }
    if order != 91 { return 2 }
    fix grid = new #[[1, 2], [3, 4]]
    for row in grid { row#[0] += 10 }
    var rows: i64 = 0
    rev row in grid {
        fix v = row.[0]
        rows = rows * 100 + v
    }
    if rows != 1311 { return 3 }
    fix empty: #[i64] = new #[]
    for value in empty { return 4 }
    rev value in empty { return 5 }
    fix narrow = new #[0; 2, 0]
    var count: i64 = 0
    for row in narrow { if len row != 0 { return 6 }
        count += 1 }
    if count != 2 { return 7 }
    fix node = new Node { n: 5 }
    fix pointers = new #[node]
    for p in pointers { p#n += 1 }
    if node.n != 6 { return 8 }
    return 0
}
"#,
        "array-iteration.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn array_views_and_iteration_keep_the_original_storage_lifetime() {
    for update in [
        "fix row = a.[0]\nrow#[0] = new Node { n: 1, next: nil }",
        "fix view = a.[0:1]\nview.[0]#[0] = new Node { n: 1, next: nil }",
        "for row in a { row#[0] = new Node { n: 1, next: nil } }",
        "fix row = a.[0]\nfor p in row { let found = p { found#next = new Node { n: 1, next: nil } } }",
    ] {
        let source = format!(
            "struct Node {{ n: i64, next: mut? Node }}\nfn main() -> u8 {{ fix node = new Node {{ n: 0, next: nil }}\nfix a: #[mut? Node; _, _] = new #[node; 1, 2]\narena {{ {update} }}\nreturn 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "arena.store", "{update}: {error}");
    }
    for body in [
        "fix a = new #[1; 2, 3]\nresult = a.[0]",
        "fix a = new #[1; 3]\nresult = a.[:]",
    ] {
        let source = format!(
            "fn main() -> u8 {{ var result: #[i64]? = nil\narena {{ {body} }}\nreturn 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "arena.store", "{body}: {error}");
    }
}

#[test]
fn array_views_reject_missing_or_redundant_handlers_and_whole_view_writes() {
    for (body, rule) in [
        ("fix row = a.[2]", "array.access-handler"),
        ("fix slice = a.[0:3]", "array.access-handler"),
        ("a#[0] = a.[0]", "array.write"),
        ("a#[0:1] = a", "array.write"),
        ("a#[:] = a", "array.write"),
        ("a.[:] = a", "array.write"),
        ("a.[0:1] = a", "array.write"),
        ("(a.[:]) = a", "array.write"),
        ("fix row = a.[true] or panic \"row\"", "array.index"),
        ("fix slice = a.[0:false] or panic \"slice\"", "array.index"),
        ("var b = a\nfor row in b {}", "for.bound-reference"),
    ] {
        let source = format!("fn main() -> u8 {{ fix a = new #[0; 2, 3]\n{body}\nreturn 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), rule, "{body}: {error}");
    }
    for body in ["fix full = a.[:] or a", "fix full = a.[:].[:] or a"] {
        let source = format!("fn main() -> u8 {{ fix a = new #[0; 2, 3]\n{body}\nreturn 0 }}");
        let warnings = suz::check_source(source.as_bytes(), suz::ArithmeticMode::Default).unwrap();
        assert_eq!(warnings.len(), 1, "{body}: {warnings:?}");
        assert_eq!(warnings[0].rule(), "array.redundant-handler");
    }
}

#[test]
fn array_slices_validate_wide_bounds_and_allow_empty_ends() {
    let output = compile_and_run_c23(
        r#"
fn length(a: #[i64], start: i64, end: u64) -> i64 {
    fix view = a.[start:end] or return -1
    return len view as i64
}
fn main() -> u8 {
    fix a = new #[1, 2, 3]
    if length(a, -1, 2) != -1 { return 1 }
    if length(a, 0, 18446744073709551615) != -1 { return 2 }
    if length(a, 2, 1) != -1 { return 3 }
    if length(a, 3, 3) != 0 { return 4 }
    fix left = a.[:2]
    fix right = a.[2:]
    if len left != 2 || len right != 1 { return 5 }
    fix empty: #[i64] = new #[]
    fix full = empty.[:]
    if len full != 0 { return 6 }
    return 0
}
"#,
        "array-slice-bounds.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn array_views_and_iteration_preserve_callable_origins() {
    for binding in [
        "fix row = fs.[0]\nfor f in row.[:] { h#p = f(a) }",
        "for row in fs { for f in row { h#p = f(a) } }",
    ] {
        let source = format!(
            r#"
struct Node {{ n: i64 }}
struct Holder {{ p: mut? Node }}
fnshape Read(#[mut? Node]) -> mut? Node
fn get(a: #[mut? Node]) -> mut? Node {{
    fix p = a.[0] or return nil
    return p
}}
fn main() -> u8 {{
    fix h = new Holder {{ p: nil }}
    arena {{
        fix a: #[mut? Node] = new #[new Node {{ n: 1 }}]
        fix fs: #[static fn Read; _, _] = new #[fn get; 1, 1]
        {binding}
    }}
    return 0
}}
"#
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "arena.store", "{binding}: {error}");
    }
}

#[test]
fn array_indices_keep_independent_arithmetic_types_across_accesses() {
    let output = compile_and_run_c23(
        r#"
fn f(g: #[u8; _, _], y: i32, x: i32) -> u8 {
    fix s = (g.[y - 1].[x] or 0) + (g.[y + 1].[x] or 0)
    fix t = (g.[y].[x - 1] or 0) + (g.[y].[x + 1] or 0)
    return s + t
}
fn slices(g: #[u8; _, _], y: i32, x: i32) -> u8 {
    return (g.[y - 1:y].[0].[x] or 0) + (g.[y:y + 1].[0].[x] or 0)
}
fn main() -> u8 {
    fix g = new #[1 as u8; 4, 4]
    if f(g, 1, 1) != 4 { return 1 }
    if slices(g, 1, 1) != 2 { return 2 }
    return 0
}
"#,
        "array-index-types.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn array_dimension_names_and_loop_bounds_control_handlers() {
    let output = compile_and_run_c23(
        r#"
fn diff(a: #[i64; n], out: #[i64; n]) -> void {
    for i in 1..<n { out#[i - 1] = a.[i] - a.[i - 1] }
}
fn rows(a: #[i64; h, w]) -> void {
    for y in 0..<h { for x in 0..<w { a.[y]#[x] += 1 } }
}
fn main() -> u8 {
    fix a = new #[1, 4, 9]
    fix out: #[i64; size] = new #[0; 3]
    diff(a, out)
    if out.[0] != 3 || out.[1] != 5 || size != 3 { return 1 }
    fix grid = new #[0; 2, 3]
    rows(grid)
    fix row = grid.[1]
    fix view = row.[1:3]
    rev i in 0..<len view { view#[i] += view.[i] }
    if grid.[1].[2] != 2 { return 2 }
    return 0
}
"#,
        "array-dimensions.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn constant_array_references_do_not_allocate_or_initialize_per_call() {
    // 実行結果だけでは、呼び出すたびの再確保や要素のコピーを検出できない。
    let c = suz::compile(
        br#"
fix TABLE: .[i32] = .[7; 1024]
fn table() -> .[i32] { return TABLE }
fn main() -> u8 {
    fix a = table()
    fix b = table()
    return ifexpr len a == len b { 0 } else { 1 }
}
"#,
    )
    .unwrap();
    assert!(
        !c.contains("suz_arena_alloc("),
        "constant array uses arena allocation"
    );
    assert_eq!(
        c.matches("static int32_t const suz_constant_array_")
            .count(),
        1,
        "constant array storage is duplicated"
    );
    assert!(!c.contains("memcpy("), "constant array elements are copied");
}

#[test]
fn mixed_constant_arrays_initialize_only_nonliteral_elements_at_startup() {
    // 値が同じでも、静的に置ける要素を起動時に代入し直すと起動の処理量が増える。
    let c = suz::compile(
        br#"
fix VALUES: .[f64] = .[1.0, nan, inf]
fn values() -> .[f64] { return VALUES }
fn main() -> u8 { fix a = values()
    return ifexpr len a == 3 { 0 } else { 1 }
}
"#,
    )
    .unwrap();
    let main = c.split("int main(void)").nth(1).unwrap();
    assert_eq!(
        main.matches("] = suz_f64(").count(),
        2,
        "nonfinite elements must be restored once"
    );
    assert!(
        !main.contains("[0] ="),
        "finite element is assigned at startup"
    );
    assert!(
        c.contains("[0] = (0x10000000000000p-52)"),
        "finite element has no static initializer"
    );
}

#[test]
fn readonly_conversion_evaluates_once_without_allocating_or_copying_elements() {
    // 実行結果だけでは変換時の余計な確保を検出できないので、生成Cも確認する。
    let source = r#"
struct Counter { calls: i32 }
struct Hold { values: #[i32] }
fn get(c: mut Counter, a: #[i32]) -> #[i32] {
    c#calls += 1
    return a
}
fn holder(c: mut Counter, a: #[i32]) -> Hold {
    c#calls += 1
    return Hold { values: a }
}
fn main() -> u8 {
    fix c = new Counter { calls: 0 }
    fix a: #[i32] = new #[7; 1024]
    fix read: .[i32] = get(c, a)
    a#[0] = 9
    if c.calls != 1 { return 1 }
    if (read.[0] or 0) != 9 { return 2 }
    fix field: .[i32] = holder(c, a).values
    if c.calls != 2 { return 3 }
    if (field.[0] or 0) != 9 { return 4 }
    return 0
}
"#;
    let c = suz::compile(source.as_bytes()).unwrap();
    assert_eq!(
        c.matches("suz_arena_alloc(").count(),
        2,
        "conversion allocates storage"
    );
    assert!(!c.contains("memcpy("), "conversion copies elements");
    let output = compile_and_run_c23(source, "readonly-conversion.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn readonly_conversion_reads_local_and_field_descriptors_directly() {
    // 最適化なしでも、単純な参照の変換にディスクリプター全体の一時コピーを入れない。
    let c = suz::compile(
        br#"
struct Hold { values: #[i32] }
struct Nested { hold: Hold }
fn parameter(a: #[i32; _, _]) -> .[i32; _, _] { return a }
fn local(a: #[i32]) -> .[i32] {
    var current = a
    var result: .[i32] = current
    current = a
    result = current
    return result
}
fn field(h: Nested) -> .[i32] { return h.hold.values }
fn indirect(h: mut Nested) -> .[i32] { return h.hold.values }
fn main() -> u8 { return 0 }
"#,
    )
    .unwrap();
    assert!(
        !c.contains("suz_cmp_"),
        "simple conversion creates a temporary"
    );
    assert!(c.contains("(suz_readonly_array_"), "conversion is missing");
}

#[test]
fn array_reference_value_storage_replaces_reference_without_copying_elements() {
    let source = r#"
fn replace_ro(slot: mut .[i32]) -> void {
    #slot = .[30, 40, 50]
}
fn replace_rw(slot: mut #[i32], next: #[i32]) -> void {
    #slot = next
}
fn main() -> u8 {
    fix initial: .[i32] = .[10, 20]
    fix ro_slot: mut .[i32] = new initial
    replace_ro(ro_slot)
    fix ro_items: .[i32] = .ro_slot
    if len ro_items != 3 || (ro_items.[0] or 0) != 30 || (ro_items.[2] or 0) != 50 { return 1 }
    if initial.[0] != 10 { return 2 }

    fix first: #[i32] = new #[1, 2]
    fix second: #[i32] = new #[7, 8, 9]
    fix rw_slot: mut #[i32] = new first
    (.rw_slot)#[0] = 99
    if first.[0] != 99 { return 3 }
    replace_rw(rw_slot, second)
    if first.[0] != 99 { return 4 }
    fix current: #[i32] = .rw_slot
    if len current != 3 || (current.[1] or 0) != 8 { return 5 }
    return 0
}
"#;
    let c = suz::compile(source.as_bytes()).unwrap();
    assert!(
        !c.contains("memcpy("),
        "storing array reference copies elements"
    );
    let output = compile_and_run_c23(source, "array-ref-storage.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");

    // 内側の権限を変える変換 mut #[i32] -> mut .[i32] は拒否する。
    let invariance = b"fn main() -> u8 {\n    fix a: #[i32] = new #[1]\n    fix q: mut #[i32] = new a\n    fix p: mut .[i32] = q\n    return 0\n}";
    let err = suz::compile(invariance).unwrap_err();
    assert_eq!(err.rule(), "type.expected", "{err}");

    // 内側arenaの配列参照を外側のスロットへ差し替える操作は寿命検査で拒否する。
    for escape in [
        "fn main() -> u8 {\n    fix outer: #[i32] = new #[1]\n    fix slot: mut #[i32] = new outer\n    arena {\n        fix inner: #[i32] = new #[2]\n        #slot = inner\n    }\n    return 0\n}",
        "fn main() -> u8 {\n    fix outer: .[i32] = .[1]\n    fix slot: mut .[i32] = new outer\n    arena {\n        fix inner: #[i32] = new #[2]\n        #slot = inner\n    }\n    return 0\n}",
    ] {
        let err = suz::compile(escape.as_bytes()).unwrap_err();
        assert_eq!(err.rule(), "arena.store", "{escape}\n{err}");
    }
}

#[test]
fn readonly_array_arena_allocation_supports_runtime_values_and_enforces_lifetimes() {
    let source = r#"
struct Counter { v: i64 }
fn tick(c: mut Counter, x: i64) -> i64 {
    c#v = c.v * 10 + x
    return x
}
fn build(x: i32, n: len_t) -> .[i32] {
    return new .[x; n]
}
fn main() -> u8 {
    fix c = new Counter { v: 0 }
    fix a = new .[tick(c, 1), tick(c, 2), tick(c, 3)]
    if c.v != 123 || a.[0] != 1 || a.[2] != 3 { return 1 }

    fix rep = build(7, 4)
    if len rep != 4 || (rep.[3] or 0) != 7 { return 2 }

    fix zero = new .[0 as i32; 3]
    if zero.[0] != 0 || zero.[2] != 0 { return 3 }

    fix empty: .[i32] = new .[]
    if len empty != 0 { return 4 }

    fix x: i32 = 5
    fix grid = new .[[x, x + 1], [x + 2, x + 3]]
    if grid.[1].[1] != 8 { return 5 }

    // 静的な定数配列とarenaの読み取り専用配列を同じ変数で切り替えられる。
    var cur: .[i32] = .[10, 20]
    if (cur.[0] or 0) != 10 { return 6 }
    cur = a_sub(rep)
    if (cur.[0] or 0) != 7 { return 7 }
    return 0
}
fn a_sub(v: .[i32]) -> .[i32] { return v }
"#;
    let c = suz::compile(source.as_bytes()).unwrap();
    assert!(
        !c.contains("memcpy("),
        "readonly arena allocation copies elements"
    );
    let output = compile_and_run_c23(source, "readonly-arena-alloc.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");

    // 要素への書き込みは拒否する。
    let write_err = suz::compile(
        b"fn main() -> u8 {\n    fix x: i64 = 1\n    fix a = new .[x, 2]\n    a#[0] = 3\n    return 0\n}",
    )
    .unwrap_err();
    assert_eq!(write_err.rule(), "array.readonly", "{write_err}");

    // arena外への不正な持ち出しは寿命検査で拒否する。
    let escape_err = suz::compile(
        b"fn main() -> u8 {\n    var saved: .[i64] = .[]\n    fix x: i64 = 1\n    arena {\n        saved = new .[x, 2]\n    }\n    return 0\n}",
    )
    .unwrap_err();
    assert_eq!(escape_err.rule(), "arena.store", "{escape_err}");

    // 要素に含まれる内側arena参照の持ち出しも拒否する。
    let elem_escape = suz::compile(
        b"struct Boxed { v: i32 }\nfn main() -> u8 {\n    var saved: .[mut Boxed] = new .[]\n    arena {\n        fix b = new Boxed { v: 1 }\n        saved = new .[b]\n    }\n    return 0\n}",
    )
    .unwrap_err();
    assert_eq!(elem_escape.rule(), "arena.store", "{elem_escape}");

    // 裸の .[...] に実行時の値を含めると定数配列の規則で拒否する。
    let bare_err = suz::compile(
        b"fn main() -> u8 {\n    fix x: i64 = 1\n    fix a = .[x, 2]\n    return 0\n}",
    )
    .unwrap_err();
    assert_eq!(bare_err.rule(), "array.constant", "{bare_err}");
}

#[test]
fn nilable_readonly_arrays_support_let_conversions_branches_generics_and_nested_pointers() {
    let source = r#"
fn first_or[T](a: .[T]?, fallback: T) -> T {
    let items = a {
        return items.[0] or fallback
    }
    return fallback
}
fn maybe_view(flag: bool, rw: #[i32], opt_rw: #[i32]?) -> .[i32]? {
    if flag { return rw }
    return opt_rw
}
fn main() -> u8 {
    // 空配列とnilを区別する。
    fix empty_opt: .[i32]? = .[]
    let e = empty_opt {
        if len e != 0 { return 1 }
    } else { return 2 }

    fix absent: .[i32]? = nil
    let found = absent { return 3 }

    // .[T] -> .[T]?, #[T] -> .[T]?, #[T]? -> .[T]? を初期化・代入・引数・戻り値・分岐・型推論で揃える。
    fix ro: .[i32] = .[11, 22]
    fix rw: #[i32] = new #[33, 44]
    fix rw_opt: #[i32]? = rw
    fix rw_nil: #[i32]? = nil

    var cur: .[i32]? = ro
    if first_or(cur, 0) != 11 { return 4 }
    cur = rw
    if first_or(cur, 0) != 33 { return 5 }
    cur = rw_opt
    if first_or(cur, 0) != 33 { return 6 }
    cur = rw_nil
    if first_or(cur, 99) != 99 { return 7 }

    if first_or(maybe_view(true, rw, rw_nil), 0) != 33 { return 8 }
    if first_or(maybe_view(false, rw, rw_nil), 88) != 88 { return 9 }

    // 分岐の型合わせ: .[i32] と nil -> .[i32]?、#[i32]? と .[i32] -> .[i32]?
    fix b1 = ifexpr true { ro } else { nil }
    if first_or(b1, 0) != 11 { return 10 }
    fix b2 = ifexpr false { rw_opt } else { ro }
    if first_or(b2, 0) != 11 { return 11 }
    fix b3 = ifexpr true { rw_nil } else { ro }
    if first_or(b3, 77) != 77 { return 12 }

    // mut? .[T], mut .[T]?, mut? .[T]? の区別と、letが最外側だけを外すことの確認。
    fix slot_opt_inner: mut .[i32]? = new absent
    #slot_opt_inner = ro
    let v1 = .slot_opt_inner {
        if (v1.[1] or 0) != 22 { return 13 }
    } else { return 14 }

    fix both_opt: mut? .[i32]? = slot_opt_inner
    let nonnil_slot = both_opt {
        #nonnil_slot = nil
        let inner_val = .nonnil_slot { return 15 }
    } else { return 16 }

    return 0
}
"#;
    let output = compile_and_run_c23(source, "nilable-readonly-array.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");

    // 未確認の .[T]? への直接アクセスは拒否する。
    for (bad, rule) in [
        (
            "fn main() -> u8 {\n    fix a: .[i32]? = nil\n    fix n = len a\n    return 0\n}",
            "len.targets",
        ),
        (
            "fn main() -> u8 {\n    fix a: .[i32]? = nil\n    fix n = a.[0] or 0\n    return 0\n}",
            "array.value",
        ),
    ] {
        let err = suz::compile(bad.as_bytes()).unwrap_err();
        assert_eq!(err.rule(), rule, "{bad}\n{err}");
    }

    // mut? .[i32]? をletで確認しても内側の ? は外れない。
    let inner_still_nil = suz::compile(
        b"fn main() -> u8 {\n    fix a: .[i32]? = nil\n    fix p: mut? .[i32]? = new a\n    let q = p {\n        fix x: .[i32] = .q\n    }\n    return 0\n}",
    )
    .unwrap_err();
    assert_eq!(inner_still_nil.rule(), "type.expected", "{inner_still_nil}");

    // 寿命検査: 内側arenaの .[T]? を外側へ保存できない。
    let escape = suz::compile(
        b"fn main() -> u8 {\n    var saved: .[i32]? = nil\n    arena {\n        fix inner: #[i32] = new #[1]\n        saved = inner\n    }\n    return 0\n}",
    )
    .unwrap_err();
    assert_eq!(escape.rule(), "arena.store", "{escape}");
}

#[test]
fn constant_array_elements_stay_distinct_from_rows_and_diagnostics_are_specific() {
    let source = r#"
fix JAGGED: .[.[i32]] = .[.[1, 2], .[3]]
fn main() -> u8 {
    arena {
        fix a: #[.[i32]] = new #[.[1, 2], .[3, 4]]
        if (a.[0].[1] or 0) != 2 || (a.[1].[0] or 0) != 3 { return 1 }
        fix b = new #[.[10, 20], .[30]]
        if (b.[0].[1] or 0) != 20 || (b.[1].[0] or 0) != 30 { return 2 }
        fix rep: #[.[i32]] = new #[.[7, 8]; 2]
        if (rep.[1].[0] or 0) != 7 { return 3 }
        fix empty_rep: #[.[i32]] = new #[.[]; 2]
        if len empty_rep != 2 || len empty_rep.[0] != 0 { return 4 }
        fix grid: #[.[i32]; _, _] = new #[[.[1], .[2]], [.[3], .[4]]]
        if (grid.[1].[0].[0] or 0) != 3 || (grid.[1].[1].[0] or 0) != 4 { return 5 }
        if JAGGED.[0].[1] != 2 || JAGGED.[1].[0] != 3 { return 6 }
    }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "const-array-elements.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");

    // 要素の定数配列は読み取り専用であり、a.[0]#[0] = 9 を拒否する。
    let ro_elem_write = suz::compile(
        b"fn main() -> u8 {\n    arena {\n        fix a = new #[.[1 as i32, 2 as i32]]\n        a.[0]#[0] = 9\n    }\n    return 0\n}",
    )
    .unwrap_err();
    assert_eq!(ro_elem_write.rule(), "array.readonly", "{ro_elem_write}");

    // 行 [...] と定数配列要素 .[...] の混在は拒否する。
    for mixed in [
        "fn main() -> u8 {\n    fix a = new #[.[1, 2], [3, 4]]\n    return 0\n}",
        "fn main() -> u8 {\n    fix a = new #[[1, 2], .[3, 4]]\n    return 0\n}",
    ] {
        let err = suz::compile(mixed.as_bytes()).unwrap_err();
        assert_eq!(err.rule(), "array.enumeration", "{mixed}\n{err}");
    }

    // 裸の #[...] には new #[...] を案内する。
    let bare =
        suz::compile(b"fn main() -> u8 {\n    fix a = #[1, 2]\n    return 0\n}").unwrap_err();
    assert_eq!(bare.rule(), "array.new", "{bare}");
    assert!(bare.message().contains("new #[...]"), "{bare}");

    // 配列・text を指すポインタへの直接添字 (.pa.[0]) には (.p).[i] を案内する。
    let ptr_index = suz::compile(
        b"fn main() -> u8 {\n    arena {\n        fix pa: mut #[i64] = new (new #[5, 6])\n        fix x = .pa.[0]\n    }\n    return 0\n}",
    )
    .unwrap_err();
    assert_eq!(ptr_index.rule(), "type.expected", "{ptr_index}");
    assert!(ptr_index.message().contains("(.p).[i]"), "{ptr_index}");

    // 配列も text も指さないポインタ (mut i64) には (.p).[i] を案内しない。
    let scalar_ptr_index = suz::compile(
        b"fn main() -> u8 {\n    arena {\n        fix mp: mut i64 = new 5\n        fix x = mp.[0]\n    }\n    return 0\n}",
    )
    .unwrap_err();
    assert_eq!(
        scalar_ptr_index.rule(),
        "type.expected",
        "{scalar_ptr_index}"
    );
    assert!(
        !scalar_ptr_index.message().contains("(.p).[i]"),
        "{scalar_ptr_index}"
    );

    // ジェネリック構造体の型引数に含まれる配列参照は #[T] / .[T] で表示する。
    let generic_diag = suz::compile(
        b"struct Boxed[T] { v: T }\nfn main() -> u8 {\n    arena {\n        fix rw: #[i32] = new #[1]\n        fix b = Boxed[.[i32]] { v: rw }\n        fix c: Boxed[#[i32]] = b\n    }\n    return 0\n}",
    )
    .unwrap_err();
    assert_eq!(generic_diag.rule(), "type.expected", "{generic_diag}");
    assert!(
        generic_diag.message().contains("Boxed[#[i32]]")
            && generic_diag.message().contains("Boxed[.[i32]]"),
        "{generic_diag}"
    );
}

#[test]
fn constant_expression_array_refs_keep_known_lengths_across_chains_and_struct_fields() {
    let source = r#"
struct Cfg { xs: .[i32] }
fix JAGGED: .[.[i32]] = .[.[1, 2], .[3]]
fix CFG = Cfg { xs: .[10, 20] }
fix XS: .[i32] = CFG.xs
fix DEEP: .[.[.[i32]]] = .[.[.[1], .[2, 3]], .[.[4]]]
fix V: i32 = JAGGED.[1].[0]

fn main() -> u8 {
    if JAGGED.[1].[0] != 3 || V != 3 { return 1 }
    if JAGGED.[0:2].[1].[0] != 3 { return 2 }
    if CFG.xs.[1] != 20 || XS.[1] != 20 { return 3 }
    if DEEP.[0].[1].[1] != 3 { return 4 }
    var sum: i32 = 0
    for i in 0..<len JAGGED.[0] { sum += JAGGED.[0].[i] }
    for i in 0..<len CFG.xs { sum += CFG.xs.[i] }
    if sum != 33 { return 5 }
    if (JAGGED.[1].[5] or 9) != 9 { return 6 }
    if (JAGGED.[5] or .[9]).[0] != 9 { return 7 }
    if (CFG.xs.[0:5] or CFG.xs).[1] != 20 { return 8 }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "const-array-ref-bounds.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");

    for code in [
        "fix JAGGED: .[.[i32]] = .[.[1, 2], .[3]]\nfn main() -> u8 {\n    fix x: i32 = JAGGED.[1].[0] or 0\n    return 0\n}",
        "fix JAGGED: .[.[i32]] = .[.[1, 2], .[3]]\nfn main() -> u8 {\n    fix r = JAGGED.[1]\n    fix x: i32 = r.[0] or 0\n    return 0\n}",
        "fix JAGGED: .[.[i32]] = .[.[1, 2], .[3]]\nfn main() -> u8 {\n    fix x: i32 = (JAGGED.[5] or .[9]).[0] or 7\n    return 0\n}",
        "struct Cfg { xs: .[i32] }\nfix CFG = Cfg { xs: .[1, 2] }\nfn main() -> u8 {\n    fix x: i32 = CFG.xs.[1] or 0\n    return 0\n}",
    ] {
        let warnings = suz::check_source(code.as_bytes(), suz::ArithmeticMode::Default).unwrap();
        assert_eq!(warnings.len(), 1, "{code}\n{warnings:?}");
        assert_eq!(warnings[0].rule(), "array.redundant-handler", "{code}");
    }

    for (code, rule) in [
        (
            "fix JAGGED: .[.[i32]] = .[.[1, 2], .[3]]\nfn main() -> u8 {\n    fix x: i32 = JAGGED.[1].[5]\n    return 0\n}",
            "array.constant-index",
        ),
        (
            "fn main() -> u8 {\n    arena {\n        fix a: .[.[i32]] = new .[.[1, 2], .[3]]\n        fix x: i32 = a.[1].[0]\n    }\n    return 0\n}",
            "array.access-handler",
        ),
        (
            "fn take(rows: .[.[i32]]) -> i32 {\n    return rows.[0].[0]\n}\nfn main() -> u8 { return 0 }",
            "array.access-handler",
        ),
        (
            "fix JAGGED: .[.[i32]] = .[.[1, 2], .[3]]\nfn main() -> u8 {\n    var i: i64 = 1\n    fix x: i32 = JAGGED.[i].[0]\n    return 0\n}",
            "array.access-handler",
        ),
    ] {
        let err = suz::compile(code.as_bytes()).unwrap_err();
        assert_eq!(err.rule(), rule, "{code}\n{err}");
    }

    // トップレベルの定数配列への書き込みは、連鎖や構造体フィールドを経ても [array.readonly] で拒否する。
    for body in [
        "A#[0] = 5",
        "A#[0] += 5",
        "JAGGED.[0]#[0] = 5",
        "JAGGED#[0] = .[5]",
        "CFG.xs#[0] = 5",
        "G.[0]#[1] = 5",
    ] {
        let source = format!(
            "struct Cfg {{ xs: .[i32] }}\nfix A: .[i32] = .[1, 2]\nfix JAGGED: .[.[i32]] = .[.[1, 2], .[3]]\nfix G: .[i32; _, _] = .[[1, 2], [3, 4]]\nfix CFG = Cfg {{ xs: .[1, 2] }}\nfn main() -> u8 {{\n    {body}\n    return 0\n}}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "array.readonly", "{body}: {error}");
    }

    // 途中の段が範囲外になる連鎖の定数式・リテラル根・定数参照の同一性を検証する。
    let extra_ok = r#"
struct Cfg { xs: .[i32] }
fix A: .[i32] = .[1, 2]
fix JAGGED: .[.[i32]] = .[.[1, 2], .[3]]
fix CFG = Cfg { xs: .[10, 20] }
fix T: .[.[i32]] = .[A, A]
fix K: i64 = 7
fix V: i32 = JAGGED.[5].[0] or 9
fix W: i32 = A.[0:5].[0] or 8
fix R: .[i32] = JAGGED.[5].[0:1] or A

fn main() -> u8 {
    if V != 9 || W != 8 || R.[1] != 2 { return 1 }
    fix arr: .[i32] = .[0; JAGGED.[5].[0] or 2]
    if len arr != 2 { return 2 }
    if (JAGGED.[5].[0:1] or A).[1] != 2 { return 3 }
    if .[.[K], .[1 + 2, 3]].[1].[0] != 3 { return 4 }
    var t: i32 = 0
    for i in 0..<len CFG.xs { t += CFG.xs.[i] }
    for i in 0..<len T.[0] { t += T.[1].[i] }
    for i in 0..<len JAGGED.[0:1].[0] { t += JAGGED.[0:1].[0].[i] }
    for i in 0..<len A.[:] { t += A.[:].[i] }
    for i in 0..<len A.[0:2] { t += A.[0:2].[i] }
    if t != 42 { return 5 }
    return 0
}
"#;
    let output = compile_and_run_c23(extra_ok, "const-array-ref-extra.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");

    let code = "fix K: i64 = 7\nfn main() -> u8 {\n    fix x = .[.[K], .[1 + 2, 4]].[0].[1]\n    return 0\n}";
    let err = suz::compile(code.as_bytes()).unwrap_err();
    assert_eq!(err.rule(), "array.constant-index", "{code}\n{err}");
}

#[test]
fn top_level_constants_and_constant_arrays_support_nilable_readonly_arrays_and_nil() {
    let source = r#"
struct Opt { xs: .[i32]? }
struct Node { value: i32, next: mut? Node }
struct Wrap { d: .[#[i32]]? }
fix A: .[i32] = .[1, 2]
fix N1: .[i32]? = .[1, 2]
fix N2: .[i32]? = nil
fix N3: .[i32]? = A
fix N4: .[i32]? = N2
fix P1: mut? i32 = nil
fix P2: ref? i32 = P1
fix W1: #[i32]? = nil
fix W2: .[i32]? = W1
fix O1 = Opt { xs: .[1, 2] }
fix O2 = Opt { xs: nil }
fix O3 = Opt { xs: A }
fix O4 = O1 with { xs: nil }
fix O5 = O2 with { xs: .[7, 8] }
fix EMPTY = Node { value: 9, next: nil }
fix NA: .[.[i32]?] = .[.[1, 2], nil]
fix N5: .[i32]? = NA.[5] or nil
fix REP: .[.[i32]?] = .[nil; 3]
fix PTRS: .[mut? i32] = .[nil, P1]
fix WRS: .[#[i32]?] = .[nil, W1]
fix RW_OPT_ARR: .[.[#[i32]]?] = .[nil]
fix WRAPS: .[Wrap] = .[Wrap { d: nil }]
fix OS: .[Opt] = .[Opt { xs: .[1, 2] }, Opt { xs: nil }]
fix NODES: .[Node] = .[EMPTY]

fn main() -> u8 {
    let a = N1 { if a.[1] != 2 { return 1 } } else { return 2 }
    let a = N2 { return 3 }
    let a = N3 { if a.[0] != 1 { return 4 } } else { return 5 }
    let a = N4 { return 6 }
    let a = N5 { return 7 }
    let p = P1 { return 23 }
    let p = P2 { return 24 }
    let w = W1 { return 25 }
    let w = W2 { return 26 }
    let a = O1.xs { if a.[1] != 2 { return 8 } } else { return 9 }
    let a = O2.xs { return 10 }
    let a = O3.xs { if a.[0] != 1 { return 11 } } else { return 12 }
    let a = O4.xs { return 13 }
    let a = O5.xs { if a.[1] != 8 { return 14 } } else { return 15 }
    if EMPTY.value != 9 { return 27 }
    let n = EMPTY.next { return 28 }
    let a = NA.[0] { if a.[1] != 2 { return 16 } } else { return 17 }
    let a = NA.[1] { return 18 }
    let a = REP.[2] { return 19 }
    let p = PTRS.[1] { return 29 }
    let w = WRS.[1] { return 30 }
    let r = RW_OPT_ARR.[0] { return 33 }
    let d = WRAPS.[0].d { return 34 }
    let a = OS.[0].xs { if a.[0] != 1 { return 20 } } else { return 21 }
    let a = OS.[1].xs { return 22 }
    if NODES.[0].value != 9 { return 31 }
    let n = NODES.[0].next { return 32 }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "const-nilable-readonly.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");

    // 同じ内容の定数配列は非nil・nil許容で静的データを共有する。
    let dedup = suz::compile(
        b"fix A: .[i32] = .[1, 2]\nfix N: .[i32]? = .[1, 2]\nfn main() -> u8 {\n    let a = N { if A.[0] + a.[1] != 3 { return 1 } }\n    return 0\n}",
    )
    .unwrap();
    assert_eq!(
        dedup
            .matches("static int32_t const suz_constant_array_")
            .count(),
        1,
        "{dedup}"
    );

    for (code, rule) in [
        (
            "fix N: .[i32]? = .[1, 2]\nfn main() -> u8 { fix x = N.[0] or 0\nreturn 0 }",
            "array.value",
        ),
        (
            "fix N: .[i32]? = .[1, 2]\nfn main() -> u8 { fix x = len N\nreturn 0 }",
            "len.targets",
        ),
        (
            "fix N: .[i32]? = .[1, 2]\nfix A: .[i32] = N\nfn main() -> u8 { return 0 }",
            "type.expected",
        ),
        (
            "fix P: mut? i32 = new (1 as i32)\nfn main() -> u8 { return 0 }",
            "const.expression",
        ),
        (
            "fix R: ref? i32 = new (1 as i32)\nfn main() -> u8 { return 0 }",
            "const.expression",
        ),
        (
            "fix W: #[i32]? = new #[1]\nfn main() -> u8 { return 0 }",
            "const.expression",
        ),
        (
            "fix BAD: .[#[i32]] = .[]\nfn main() -> u8 { return 0 }",
            "array.constant",
        ),
        (
            "fix BAD: .[mut i32] = .[]\nfn main() -> u8 { return 0 }",
            "array.constant",
        ),
        (
            "fix BAD: .[.[#[i32]]?] = .[.[]]\nfn main() -> u8 { return 0 }",
            "array.constant",
        ),
        (
            "struct Wrap { d: .[#[i32]]? }\nfix BAD: .[Wrap] = .[Wrap { d: .[] }]\nfn main() -> u8 { return 0 }",
            "array.constant",
        ),
    ] {
        let err = suz::compile(code.as_bytes()).unwrap_err();
        assert_eq!(err.rule(), rule, "{code}\n{err}");
    }
}

#[test]
fn enum_indexed_arrays_cover_init_access_lifetime_and_rejections() {
    // 1回評価・共有参照・フィールド更新・nil許容・定数配列の実行検証。
    let valid = r#"
enum Wave: u8 { Saw, Square, Triangle }
struct Boxed { value: i32 }
fn make_ref(calls: mut i32) -> mut Boxed {
    #calls = .calls + 1 or 0
    return new Boxed { value: 10 }
}
fn main() -> u8 {
    fix calls = new (0 as i32)
    fix refs: #[mut Boxed; Wave] = new #[make_ref(calls); Wave]
    if .calls != 1 { return 1 }
    refs.[Wave.Saw]#value = 42
    if refs.[Wave.Triangle].value != 42 { return 2 }

    fix items: #[Boxed; Wave] = new #[Boxed { value: 1 }; Wave]
    items.[Wave.Square]#value = 99
    if items.[Wave.Saw].value != 1 || items.[Wave.Square].value != 99 { return 3 }

    fix maybe: .[Boxed; Wave]? = items
    let present = maybe {
        if present.[Wave.Square].value != 99 { return 5 }
    } else {
        return 4
    }

    fix empty_ref: #[i32; Wave]? = nil
    let p = empty_ref {
        return 6
    } else {
        return 0
    }
}
"#;
    let output = compile_and_run_c23(valid, "enum-arrays.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");

    // 各種拒否ケース。
    for (code, rule) in [
        // 整数添字・0..<len a 添字・別列挙型添字
        (
            "enum Wave: u8 { Saw, Square }\nfn main() -> u8 {\nfix a = .[1; Wave]\nfix v = a.[0] or 0\nreturn 0\n}",
            "array.enum-index",
        ),
        (
            "enum Wave: u8 { Saw, Square }\nfn main() -> u8 {\nfix a = .[1; Wave]\nfor i in 0..<len a { fix v = a.[i] or 0 }\nreturn 0\n}",
            "array.enum-index",
        ),
        (
            "enum Wave: u8 { Saw, Square }\nenum Other: u8 { A, B }\nfn main() -> u8 {\nfix a = .[1; Wave]\nfix v = a.[Other.A]\nreturn 0\n}",
            "array.enum-index",
        ),
        // 異種キー・整数キー・キーなし初期化 (.[0; 2], .[])
        (
            "enum Wave: u8 { Saw, Square }\nenum Other: u8 { A, B }\nfn main() -> u8 {\nfix a = .[Wave.Saw: 1, Other.B: 2]\nreturn 0\n}",
            "array.enum-init",
        ),
        (
            "enum Wave: u8 { Saw, Square }\nfn main() -> u8 {\nfix a = .[0: 1, 1: 2]\nreturn 0\n}",
            "array.enum-init",
        ),
        (
            "enum Wave: u8 { Saw, Square }\nfn main() -> u8 {\nfix a: .[i32; Wave] = .[0; 2]\nreturn 0\n}",
            "array.enum-init",
        ),
        (
            "enum Wave: u8 { Saw, Square }\nfn main() -> u8 {\nfix a: .[i32; Wave] = .[]\nreturn 0\n}",
            "array.enum-init",
        ),
        // 要求型と異なる列挙型
        (
            "enum Wave: u8 { Saw, Square }\nenum Other: u8 { A, B }\nfn main() -> u8 {\nfix a: .[i32; Wave] = .[0; Other]\nreturn 0\n}",
            "array.enum-init",
        ),
        // error 型や未定義型名・多次元
        (
            "error Err { A, B }\nfn main() -> u8 {\nfix a: .[i32; Err] = .[0; Err]\nreturn 0\n}",
            "array.enum-dimension",
        ),
        (
            "enum Wave: u8 { Saw, Square }\nfn f(a: #[f32; Wave, _]) -> void {}\nfn main() -> u8 { return 0 }",
            "array.enum-type",
        ),
        (
            "enum Wave: u8 { Saw, Square }\nfn main() -> u8 {\nfix a = new #[0; Wav]\nreturn 0\n}",
            "array.enum-dimension",
        ),
        (
            "enum Wave: u8 { Saw, Square }\nfn main() -> u8 {\nfix a = new #[0; Wave, 3]\nreturn 0\n}",
            "array.enum-init",
        ),
        (
            "enum Wave: u8 { Saw, Square }\nfn main() -> u8 {\nfix a = .[[1, Wave.Saw: 2]]\nreturn 0\n}",
            "array.enumeration",
        ),
        // 寿命エスケープ拒否
        (
            "enum Wave: u8 { Saw, Square }\nfn main() -> u8 {\nvar out: #[i64; Wave]? = nil\narena {\nfix inner = new #[0; Wave]\nout = inner\n}\nreturn 0\n}",
            "arena.store",
        ),
    ] {
        let err = suz::compile(code.as_bytes()).unwrap_err();
        assert_eq!(err.rule(), rule, "{code}\n{err}");
    }
}
