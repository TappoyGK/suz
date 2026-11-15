use crate::test_util::compile_and_run_c23;

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn nested_struct_constants_and_equality_preserve_scalar_semantics() {
    let source = r#"struct Outer { inner: Inner, mode: Mode }
struct Inner { value: f32, flag: bool }
enum Mode: u8 { A, B }
const FIRST = Outer { mode: Mode.B, inner: Inner { value: 1.5, flag: true } }
const EQUAL = FIRST == Outer { inner: Inner { flag: true, value: 1.5 }, mode: Mode.B }
const BAD = Inner { value: nan, flag: false }
const UNEQUAL = BAD != BAD
const FIELD = FIRST.inner.value
fn copy(value: Outer) -> Outer { return value }
fn main() -> u8 {
    fix value = copy(FIRST)
    fix selected = ifexpr EQUAL { value } else { FIRST }
    if not EQUAL || not UNEQUAL || FIELD != 1.5 || selected != FIRST { return 1 }
    fix nested = Outer { inner: BAD, mode: Mode.A }
    if nested == nested || not (nested != nested) { return 2 }
    var calls: u8 = 0
    fix result = FIRST == nested == expr {
        calls += 1
        FIRST
    }
    if result || calls != 0 { return 3 }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "structs.suz");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn field_compound_assignment_saves_old_value_and_handles_failure() {
    let source = r#"struct Pair { x: u8, y: u8 }
struct Box { pair: Pair }
fn failing() -> u8 {
    var value = Box { pair: Pair { x: 255, y: 0 } }
    value.pair.x += expr {
        value.pair.y = 7
        255
    } + 1 or return expr {
        ifexpr value.pair.x == 255 && value.pair.y == 7 { 0 } else { 1 }
    }
    return 2
}
fn main() -> u8 {
    var value = Box { pair: Pair { x: 10, y: 0 } }
    value.pair.x += expr {
        value.pair.y = 7
        value.pair.x
    }
    if value.pair.x != 20 || value.pair.y != 7 { return 3 }
    value.pair.x = value.pair.x * 255 or 8
    if value.pair.x != 8 { return 4 }
    value.pair.x = 300 as u8 or return failing()
    return 5
}
"#;
    let output = compile_and_run_c23(source, "structs.suz");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn struct_literals_require_parentheses_in_each_control_target() {
    for body in [
        "if Item { x: 0 }.x == 0 {}",
        "while Item { x: 0 }.x == 0 { break }",
        "for i in 0..<Item { x: 1 }.x {}",
        "match Item { x: 0 }.x { _ => {} }",
        "fix x = ifexpr Item { x: 0 }.x == 0 { 0 } else { 1 }",
        "fix x = matchexpr Item { x: 0 }.x { _ => 0 }",
        "if false {} elif Item { x: 0 }.x == 0 {}",
    ] {
        let source = format!("struct Item {{ x: u8 }}\nfn main() -> u8 {{\n{body}\nreturn 0\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.message.contains("[expr.struct-in-cond]"),
            "{source}: {error}"
        );
        let grouped = source
            .replace("Item { x: 0 }", "(Item { x: 0 })")
            .replace("Item { x: 1 }", "(Item { x: 1 })");
        suz::compile(grouped.as_bytes()).unwrap();
    }
}

#[test]
fn struct_fields_reject_invalid_declarations_access_and_overlapping_writes() {
    for (declarations, body, rule) in [
        ("struct Item { x: u8, x: u8 }", "", "struct.field"),
        ("struct Item { x }", "", "struct.field"),
        ("struct Item { x: Item }", "", "struct.recursion"),
        (
            "struct Item { x: u8 }",
            "fix p = Item { z: 1 }",
            "struct.literal",
        ),
        (
            "struct Item { x: u8 }",
            "fix p = Item { x: 1 }\np.z",
            "struct.access",
        ),
        (
            "struct Item { x: u8 }",
            "var p = Item { x: 1 }\np.z = 0",
            "struct.access",
        ),
        (
            "struct Item { x: u8 }",
            "fix p = Item { x: 1 }\np < p",
            "expr.operator-types",
        ),
        (
            "struct Item { x: u8 }",
            "fix p = Item { x: 1 }\np + p",
            "expr.operator-types",
        ),
        (
            "struct Item { x: u8 }",
            "var p = Item { x: 1 }\np.x = expr {\np.x = 2\n0\n}",
            "assign.rhs-write",
        ),
        (
            "struct Item { x: u8 }",
            "var p = Item { x: 1 }\np.x = ifexpr false { expr {\np.x = 2\n0\n} } else { 0 }",
            "assign.rhs-write",
        ),
        (
            "struct Item { x: u8 }\nconst P = Item { x: 1 / 0 }",
            "",
            "const.evaluation",
        ),
        (
            "struct Item { x: u8 }\nconst P = Item { x: P.x }",
            "",
            "const.initializer",
        ),
    ] {
        let source = format!("{declarations}\nfn main() -> u8 {{\n{body}\nreturn 0\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(
            error.kind,
            suz::ErrorKind::InvalidSource,
            "{source}: {error}"
        );
        assert!(
            error.message.contains(&format!("[{rule}]")),
            "{source}: {error}"
        );
    }
}
