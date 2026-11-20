use crate::test_util::compile_and_run_c23;

#[test]
fn named_needs_expand_forward_references_and_inherit_into_methods() {
    let source = r#"
@needs Ordered[T]
struct Range[T] { low: T, high: T }
fn Range[T].contains(value: T) -> bool {
    if value < self.low { return false }
    return value <= self.high
}
needs Ordered[T] {
    Less[T]
    T: <=(T) -> bool
    Less[T]
}
needs Less[U] {
    U: <(U) -> bool
}
@needs Ordered[T]
@needs T: <(T) -> bool
fn inside[T](value: T, low: T, high: T) -> bool {
    return Range[T] { low: low, high: high }.contains(value)
}
fn main() -> u8 {
    if not inside(2, 1, 3) || inside(4, 1, 3) { return 1 }
    return 0
}
"#;
    for program in [
        source.to_owned(),
        source.replace(
            "@needs Ordered[T]",
            "@needs T: <(T) -> bool\n@needs T: <=(T) -> bool",
        ),
    ] {
        let output = compile_and_run_c23(&program, "named-needs-inherit.suz");
        assert!(output.status.success(), "{output:?}");
    }
}

#[test]
fn named_needs_replace_nested_types_and_preserve_definition_names() {
    let source = r#"
struct Box[T] { value: T }
fnshape Map[T, U](T) -> U
needs Maps[T, U] {
    T: .apply(Box[U], static fn Map[U, U]) -> Box[U]
}
needs Wrapped[X, Y] {
    // 引数の順番を変えても定義側の型を使用する。
    Maps[Y, X]
}
needs IntMaps[T] {
    Maps[T, i32]
}
struct Worker { id: i32 }
fn Worker.apply(value: Box[i32], action: static fn Map[i32, i32]) -> Box[i32] {
    return Box[i32] { value: action(value.value) + self.id }
}
@needs Wrapped[U, T]
fn apply[T, U](worker: T, value: Box[U], action: static fn Map[U, U]) -> Box[U] {
    return worker.apply(value, action)
}
@needs IntMaps[T]
fn apply_int[T](worker: T, value: Box[i32], action: static fn Map[i32, i32]) -> Box[i32] {
    return worker.apply(value, action)
}
fn identity(x: i32) -> i32 { return x }
fn main() -> u8 {
    fix second = apply_int(Worker { id: 2 }, Box[i32] { value: 3 }, fn identity)
    if second.value != 5 { return 2 }
    fix result = apply(Worker { id: 2 }, Box[i32] { value: 3 }, fn identity)
    if result.value != 5 { return 1 }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "named-needs-substitution.suz");
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn named_needs_keep_method_effects_and_shape_requirements() {
    let source = r#"
error Failed { Read }
struct Number { n: i32 }
fn Number.read() -> i32 or Failed { return self.n }
fn Number#increment() -> void { self#n = self.n + 1 }
arena fn Number.copy() -> ptr Number { return new self }
needs Mutable[T] {
    T: .read() -> i32 or Failed
    T: #increment() -> void
    T: arena .copy() -> ptr T
}
@needs Mutable[T]
arena fn duplicate[T](value: T) -> ptr T {
    fix ptr copy = value.copy()
    copy#increment()
    return copy
}
@needs Mutable[T]
fnshape Read[T](T) -> i32 or Failed
@needs Mutable[T]
fn read[T](value: T) -> i32 or Failed {
    fix result = value.read() or fail
    return result
}
fn main() -> u8 {
    arena {
        fix ptr copy = duplicate(Number { n: 7 })
        fix f: static fn Read[Number] = fn read[Number]
        if (f(.copy) or 0) != 8 { return 1 }
    }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "named-needs-effects.suz");
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn named_needs_validate_unused_lists_and_reject_cycles() {
    for (declarations, rule) in [
        ("needs Empty[T] {\n}\n", "generic.needs-group"),
        (
            "needs Wrong[T] {\n@needs T: <(T) -> bool\n}\n",
            "generic.needs-group",
        ),
        ("needs Wrong[T] {\nT: <(T) -> i32\n}\n", "generic.needs"),
        ("needs Wrong[T] {\nU: <(T) -> bool\n}\n", "generic.needs"),
        ("needs Wrong[T] {\nUnknown[T]\n}\n", "generic.needs-group"),
        ("needs Cycle[T] {\nCycle[T]\n}\n", "generic.needs-expand"),
        (
            "needs First[T] {\nSecond[T]\n}\nneeds Second[U] {\nFirst[U]\n}\n",
            "generic.needs-expand",
        ),
        (
            "needs Wrong[T] {\nT: .read() -> i32\nT: .read() -> bool\n}\n",
            "generic.needs-check",
        ),
        ("needs Wrong[T] {\nT: .read() -> Unknown\n}\n", "type.name"),
        (
            "needs Wrong[T] {\nfn work() -> void {}\n}\n",
            "generic.needs-group",
        ),
        (
            "needs Wrong[T] {\nfix VALUE = 1\n}\n",
            "generic.needs-group",
        ),
    ] {
        let error =
            suz::compile(format!("{declarations}fn main() -> u8 {{ return 0 }}").as_bytes())
                .unwrap_err();
        assert!(error.to_string().contains(rule), "{declarations}\n{error}");
    }
}

#[test]
fn named_needs_are_not_types_values_or_implementations() {
    let prefix = "needs Ordered[T] {\nT: <(T) -> bool\n}\n";
    for suffix in [
        "struct Ordered { n: i32 }\nfn main() -> u8 { return 0 }",
        "fnshape Ordered(i32) -> i32\nfn main() -> u8 { return 0 }",
        "enum Ordered: u8 { Low }\nfn main() -> u8 { return 0 }",
        "needs Ordered[U] {\nU: <(U) -> bool\n}\nfn main() -> u8 { return 0 }",
        "fn unused[Ordered](x: Ordered) -> void {}\nfn main() -> u8 { return 0 }",
        "fn unused(x: Ordered[i32]) -> void {}\nfn main() -> u8 { return 0 }",
        "fn main() -> u8 { fix value = Ordered[i32]\nreturn 0 }",
        "fn main() -> u8 { needs Local[T] {\nT: <(T) -> bool\n}\nreturn 0 }",
        "@needs Ordered[T]\n\nfn unused[T](x: T) -> T { return x }\nfn main() -> u8 { return 0 }",
        "@needs Ordered[T]\nneeds Other[T] {\nT: <(T) -> bool\n}\nfn main() -> u8 { return 0 }",
    ] {
        assert!(
            suz::compile(format!("{prefix}{suffix}").as_bytes()).is_err(),
            "{suffix}"
        );
    }
}

#[test]
fn named_needs_check_arity_targets_and_concrete_requirements() {
    let prefix = "needs Ordered[T] {\nT: <(T) -> bool\n}\n";
    for (requirement, rule) in [
        ("Ordered[]", "generic.needs-group"),
        ("Ordered[T, T]", "generic.needs-group"),
        ("Ordered[i32]", "generic.needs"),
        ("Missing[T]", "generic.needs-group"),
    ] {
        let source = format!(
            "{prefix}@needs {requirement}\nfn unused[T](x: T) -> T {{ return x }}\nfn main() -> u8 {{ return 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(error.to_string().contains(rule), "{error}");
    }
    let error = suz::compile(format!("{prefix}@needs Ordered[T]\nfn less[T](a: T, b: T) -> bool {{ return a < b }}\nfn main() -> u8 {{ fix result = less(true, false)\nreturn 0 }}").as_bytes()).unwrap_err();
    assert!(error.to_string().contains("generic.needs-check"), "{error}");
}

#[test]
fn named_needs_detect_conflicts_after_parameter_substitution() {
    let prefix = "needs Reads[T, U] {\nT: .read() -> i32\nU: .read() -> bool\n}\n";
    let error = suz::compile(format!("{prefix}@needs Reads[T, T]\nfn unused[T](x: T) -> T {{ return x }}\nfn main() -> u8 {{ return 0 }}").as_bytes()).unwrap_err();
    assert!(error.to_string().contains("generic.needs-check"), "{error}");
    assert_eq!(error.line, 6, "{error:?}");
    assert_eq!(
        error.related.iter().map(|e| e.line).collect::<Vec<_>>(),
        [2, 3],
        "{error:?}"
    );
    let valid = format!(
        "{prefix}@needs Reads[T, U]\nfn unused[T, U](x: T, y: U) -> T {{ return x }}\nfn main() -> u8 {{ return 0 }}"
    );
    suz::compile(valid.as_bytes()).unwrap();
}

#[test]
fn named_needs_deduplicate_diamond_expansion() {
    let mut source = "needs Base[T] {\nT: <(T) -> bool\n}\n".to_owned();
    for i in 0..30 {
        let previous = if i == 0 {
            "Base".to_owned()
        } else {
            format!("Layer{}", i - 1)
        };
        source.push_str(&format!(
            "needs Layer{i}[T] {{\n{previous}[T]\n{previous}[T]\n}}\n"
        ));
    }
    source.push_str("@needs Layer29[T]\nfn less[T](a: T, b: T) -> bool { return a < b }\nfn main() -> u8 { if not less(1, 2) { return 1 }\nreturn 0 }");
    let output = compile_and_run_c23(&source, "named-needs-diamond.suz");
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn named_needs_resolve_long_forward_chains_without_recursive_expansion() {
    let mut source = String::new();
    for i in 0..2000 {
        source.push_str(&format!("needs Layer{i}[T] {{\nLayer{}[T]\n}}\n", i + 1));
    }
    source.push_str("needs Layer2000[T] {\nT: <(T) -> bool\n}\n@needs Layer0[T]\nfn less[T](a: T, b: T) -> bool { return a < b }\nfn main() -> u8 { return 0 }");
    suz::compile(source.as_bytes()).unwrap();
}

#[test]
fn named_needs_conflicts_report_definitions_through_nested_lists() {
    let prefix =
        "needs Aa[A, B] {\nA: +(A) -> A\nA: +(B) -> i64\n}\nneeds Outer[X, Y] {\nAa[X, Y]\n}\n";
    for (declaration, use_line) in [
        (
            "@needs Outer[T, T]\nfn unused[T](x: T) -> T { return x }\nfn main() -> u8 { return 0 }",
            9,
        ),
        (
            "@needs Outer[T, U]\nfn unused[T, U](x: T, y: U) -> T { return x }\nfn main() -> u8 { fix result = unused(1 as i32, 2 as i32)\nreturn 0 }",
            10,
        ),
    ] {
        let error = suz::compile(format!("{prefix}{declaration}").as_bytes()).unwrap_err();
        assert_eq!(error.line, use_line, "{error:?}");
        assert_eq!(
            error.related.iter().map(|e| e.line).collect::<Vec<_>>(),
            [2, 3],
            "{error:?}"
        );
    }
}
