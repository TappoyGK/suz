//! 配列の実体、境界検査の評価順、領域をまたぐ保存を実行結果と拒否例で検査する。
use crate::test_util::compile_and_run_c23;

#[test]
fn arrays_allocate_read_write_and_keep_empty_distinct_from_null() {
    let output = compile_and_run_c23(
        r#"
fn read(a: ptr [i64], i: i64) -> i64 { return a[i] or -1 }
fn cell(a: ptr [i64; _, _], y: i64, x: i64) -> i64 { return a[y][x] or -1 }
fn main() -> u8 {
    fix ptr a = new [2, 4, 6]
    a#[1] += 3
    if read(a, 1) != 7 { return 1 }
    if read(a, -1) != -1 { return 2 }
    fix ptr grid = new [[1, 2], [3, 4]]
    grid[1]#[0] = 8
    if cell(grid, 1, 0) != 8 { return 3 }
    fix ptr empty: ptr? [i64] = new []
    let p = empty { if len p != 0 { return 4 } } else { return 5 }
    fix ptr absent: ptr? [i64] = null
    let p = absent { return 6 }
    fix ptr repeated = new [9; 3]
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
fn save(a: ptr [ptr? Node], p: ptr? Node) -> void { a#[0] = p }
fn read(a: ptr [ptr? Node]) -> ptr? Node { fix ptr p = a[0] or return null
return p }
fn wrap(p: ptr? Node) -> ptr [ptr? Node] { return new [p] }
"#;
    for body in [
        "a#[0] = new Node { v: 1 }",
        "fix ptr alias = a\nalias#[0] = new Node { v: 1 }",
        "save(a, new Node { v: 1 })",
    ] {
        let source = format!(
            "{prefix}\nfn main() -> u8 {{ fix ptr a: ptr [ptr? Node] = new [null; 2]\narena {{ {body} }}\nreturn 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "arena.store", "{body}: {error}");
    }
    let source = format!(
        "{prefix}\nfn main() -> u8 {{ fix ptr p = new Node {{ v: 7 }}\nfix ptr a = wrap(p)\nlet v = read(a) {{ if v.v != 7 {{ return 1 }} }} else {{ return 2 }}\nreturn 0 }}"
    );
    let output = compile_and_run_c23(&source, "array-lifetimes.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn array_reads_short_circuit_indices_and_lower_return_handlers() {
    let output = compile_and_run_c23(
        r#"
struct Counter { v: i64 }
fn tick(c: ptr Counter, n: i64) -> i64 { c#v += 1
    return n }
fn read(a: ptr [i64; _, _], c: ptr Counter, y: i64) -> i64 {
    return a[tick(c, y)][tick(c, 0)] or tick(c, -1)
}
fn early(a: ptr [i64], i: i64) -> i64 {
    fix v = a[i] or return 42
    return v
}
fn main() -> u8 {
    fix ptr c = new Counter { v: 0 }
    fix ptr a = new [5; 2, 3]
    if read(a, c, -1) != -1 { return 1 }
    if c.v != 2 { return 2 }
    c#v = 0
    if read(a, c, 0) != 5 { return 3 }
    if c.v != 2 { return 4 }
    fix ptr b = new [1]
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
fnshape Read(ptr [i64], i64) -> i64
fn read[T](a: ptr [T], i: i64, otherwise: T) -> T { return a[i] or otherwise }
@needs T: len() -> len_t
fn count[T](a: T) -> len_t { return len a }
fn reader(a: ptr [i64], i: i64) -> i64 { return a[i] or -1 }
fn main() -> u8 {
    fix ptr a = new [Box[i32] { v: 2 }; 3]
    a[1]#v = 7
    fix b = read(a, 1, Box[i32] { v: 0 })
    if b.v != 7 { return 1 }
    if count(a) != 3 { return 2 }
    fix ptr fs: ptr [static fn Read] = new [fn reader]
    fix f = fs[0] or fn reader
    fix ptr numbers = new [8]
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
        ("", "fix ptr a = new []", "array.initializer-type"),
        ("", "fix ptr a: ptr [i32; _, _] = new []", "array.type"),
        ("", "fix ptr a = new [[1, 2], [3]]", "array.enumeration"),
        ("", "fix ptr a = new [[], []]", "array.enumeration"),
        ("", "fix ptr a = new [0; -1]", "array.layout"),
        ("", "fix ptr a = new [0; 2147483648]", "array.layout"),
        (
            "",
            "fix ptr a = new [0; 0, 2147483647, 2147483647, 2147483647]",
            "array.layout",
        ),
        (
            "",
            "fix ptr a = new [0; 2147483647, 2147483647]",
            "array.layout",
        ),
        (
            "",
            "fix small: i32 = 1\nfix ptr a = new [small, 2]",
            "array.initializer-type",
        ),
        (
            "",
            "fix ptr a: ptr [i32] = new [1]\nfix ptr b: ptr [i64] = a",
            "type.expected",
        ),
        (
            "",
            "fix ptr a: ptr? [i32] = null\nfix n = len a",
            "len.targets",
        ),
        (
            "struct Node { next: ptr [Node; _, _] }",
            "",
            "arena.recursive-struct",
        ),
    ] {
        let source = format!("{declarations}\nfn main() -> u8 {{ {body}\nreturn 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), rule, "{body}: {error}");
    }
    suz::compile(b"struct Node { next: ptr [Node] }\nfn main() -> u8 { fix root = Node { next: new [] }\nreturn 0 }").unwrap();
}

#[test]
fn array_failures_precede_later_indices_and_assignment_values() {
    for (body, reason) in [
        ("a[0]#[3] = rhs()", "dimension=2 index=3 length=3"),
        ("a[-1]#[rhs()] = rhs()", "dimension=1 index=-1 length=2"),
        (
            "fix huge: u64 = 18446744073709551615
a[0]#[huge] = rhs()",
            "dimension=2 index=18446744073709551615 length=3",
        ),
    ] {
        let source = format!(
            "fn rhs() -> i64 {{ panic `RHS evaluated` }}\nfn main() -> u8 {{ fix ptr a = new [0; 2, 3]\n{body}\nreturn 0 }}"
        );
        let output = compile_and_run_c23(&source, "array-bounds.suz");
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(reason), "{stderr}");
        assert!(!stderr.contains("RHS evaluated"), "{stderr}");
    }
}

#[test]
fn array_initializers_evaluate_once_before_dimensions_even_when_empty() {
    let output = compile_and_run_c23(
        r#"
struct Counter { v: i64 }
fn tick(p: ptr Counter, v: i64) -> i64 { p#v = p.v * 10 + v
    return v }
fn main() -> u8 {
    fix ptr p = new Counter { v: 0 }
    fix ptr a = new [tick(p, 7); tick(p, 2), tick(p, 0)]
    if p.v != 720 { return 1 }
    if len a != 2 { return 2 }
    p#v = 0
    fix ptr b = new [[tick(p, 1), tick(p, 2)], [tick(p, 3), tick(p, 4)]]
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
    for (dimensions, arguments, reason) in [
        ("a", "-1, 0", "dimension 1"),
        ("a", "2147483648, 0", "dimension 1"),
        ("a, b", "2147483647, 2147483647", "size_t"),
        ("0, a, b, a", "2147483647, 2147483647", "size_t"),
    ] {
        let source = format!(
            "fn allocate(a: i64, b: i64) -> void {{ fix ptr p = new [0; {dimensions}] }}\nfn main() -> u8 {{ allocate({arguments})\nreturn 0 }}"
        );
        let output = compile_and_run_c23(&source, "array-size.suz");
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("rt.array-size") && stderr.contains(reason),
            "{stderr}"
        );
    }
}

#[test]
fn writes_keep_the_original_element_and_read_the_old_value_before_rhs() {
    let output = compile_and_run_c23(
        r#"
struct Holder { a: ptr [i64] }
fn replace(h: ptr Holder, replacement: ptr [i64]) -> i64 {
    h#a = replacement
    return 3
}
fn overwrite(a: ptr [i64]) -> i64 { a#[0] = 20
    return 5 }
fn read(a: ptr [i64], i: i64) -> i64 { return a[i] or -1 }
fn main() -> u8 {
    fix ptr a = new [10]
    fix ptr b = new [100]
    fix ptr h = new Holder { a: a }
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
            "fix ptr fs: ptr [static fn Read] = new [fn get]\nfix f = fs[0] or return 1\nfix ptr p = f(a)"
        } else {
            "fix ptr p = get(a)"
        };
        let source = format!(
            r#"
struct Node {{ v: i64 }}
struct Holder {{ p: ptr? Node }}
fnshape Read(ptr [ptr? Node]) -> ptr? Node
fn get(a: ptr [ptr? Node]) -> ptr? Node {{
    fix ptr p = a[0] or return null
    return p
}}
fn main() -> u8 {{
    fix ptr h = new Holder {{ p: null }}
    arena {{
        fix ptr a: ptr [ptr? Node] = new [new Node {{ v: 1 }}]
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
fn read(a: ptr [ptr [i64]], y: i64, x: i64) -> i64 { return a[y][x] or -1 }
fn main() -> u8 {
    fix ptr short = new [1]
    fix ptr long = new [2, 3]
    fix ptr rows = new [short, long]
    rows[1]#[1] = 7
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
    let source = b"struct Node { v: i32 }
struct Item { p: ptr? Node, n: i32 }
fn main() -> u8 {
    arena { fix ptr a = new [Item { p: null, n: 0 }; 3]
      arena { fix ptr t = new Node { v: 2 }
        a[0]#p = t } }
    return 0 }";
    let error = suz::compile(source).unwrap_err();
    assert_eq!(error.rule(), "arena.store", "{error}");
}

#[test]
fn array_source_diagnostics_are_not_internal_errors() {
    use std::{fs, process::Command};
    let directory = crate::test_util::TemporaryDirectory::new();
    for (source, rule) in [
        (
            "fn f(a: ptr [i32; n]) -> void {}\nfn main() -> u8 { return 0 }",
            "array.dimension-bind",
        ),
        (
            "fn main() -> u8 { fix ptr a = new [0; 2, 3]\nfix ptr row = a[0] or panic `missing`\nreturn 0 }",
            "array.index",
        ),
        (
            "fn main() -> u8 { fix ptr a = new [0; 3]\nfix ptr view = a[0:2] or panic `missing`\nreturn 0 }",
            "array.slice",
        ),
        (
            "fn main() -> u8 { fix ptr a = new [0; 3]\na[0] = 1\nreturn 0 }",
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
            assert!(message.contains("not implemented yet"), "{message}");
        }
    }
}

#[test]
fn array_zero_initialization_preserves_negative_zero_and_nonzero_values() {
    let output = compile_and_run_c23(
        r#"
fn get(a: ptr [f64], i: i64) -> f64 { return a[i] or 10.0 }
fn main() -> u8 {
    fix ptr positive = new [0.0; 4]
    fix ptr negative = new [-0.0; 4]
    fix ptr filled = new [1.5; 4]
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
fn read(a: ptr [i64], i: i64) -> i64 { return a[i] or -1 }
fn main() -> u8 {
    fix ptr first = new [10]
    fix ptr second = new [20]
    var ptr current = first
    fix old = current[invoke(fn [var current, second]() -> i64 {
        current = second
        return 0
    })] or -1
    if old != 10 || read(current, 0) != 20 { return 1 }
    current = first
    fix again = current[expr { current = second
        0 }] or -1
    if again != 10 { return 2 }
    fix captured = invoke(fn [first]() -> i64 { return first[0] or -1 })
    fix blocked = expr { fix n = read(second, 0)
        (first[0] or -1) + n }
    if captured != 10 || blocked != 30 { return 3 }
    fix ptr grid = new [[10], [20]]
    var row: i64 = 0
    fix original = grid[row][expr { row = 1
        0 }] or -1
    if original != 10 { return 4 }
    return 0
}
"#,
        "array-reference-snapshots.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}
