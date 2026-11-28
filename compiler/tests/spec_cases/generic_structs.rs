use crate::test_util::compile_and_run_c23;

#[test]
fn generic_structs_preserve_nominal_types_fields_and_methods() {
    let output = compile_and_run_c23(
        r#"
struct Box[T] { value: T }
fn Box[T].get() -> T { return self.value }
fn Box[T]#replace(value: T) -> void { self#value = value }
fn wrap[T](value: T) -> Box[T] { return Box[T] { value: value } }
fix DEFAULT = Box[i32] { value: 7 }
fn main() -> u8 {
    if DEFAULT.value != 7 { return 3 }
    var box = wrap[i32](12)
    box#replace(20)
    if box.get() != 20 { return 1 }
    fix nested = Box[Box[i32]] { value: box }
    if nested.value.get() != 20 { return 2 }
    return 0
}
"#,
        "generic-structs.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn generic_recursive_structs_use_the_existing_arena_rules() {
    let output = compile_and_run_c23(
        r#"
struct Node[T] { value: T, next: mut? Node[T] }
fn node[T](value: T) -> mut Node[T] { return new Node[T] { value: value, next: nil } }
fn main() -> u8 {
    arena {
        fix first = node[i32](3)
        fix second = node[i32](4)
        first#next = second
        let n = first.next { if n.value != 4 { return 1 } }
    }
    return 0
}
"#,
        "generic-recursive-structs.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn struct_requirements_are_inherited_but_method_requirements_are_lazy() {
    let output = compile_and_run_c23(
        r#"
@needs T: ==(T) -> bool
struct Box[T] { value: T }
fn Box[T].same(value: T) -> bool { return self.value == value }
@needs T: +(T) -> T
fn Box[T].add(value: T) -> T { return self.value + value }
fn main() -> u8 {
    fix flag = Box[bool] { value: true }
    if not flag.same(true) { return 1 }
    fix n = Box[i32] { value: 3 }
    if n.add(4) != 7 { return 2 }
    return 0
}
"#,
        "generic-struct-requirements.suz",
    );
    assert!(output.status.success(), "{output:?}");
    for source in [
        "@needs T: +(T) -> T\nstruct Box[T] { value: T }\nfn main() -> u8 { fix b = Box[bool] { value: true }\nreturn 0 }",
        "struct Box[T] { value: T }\n@needs T: +(T) -> T\nfn Box[T].add(value: T) -> T { return self.value + value }\nfn main() -> u8 { fix b = Box[bool] { value: true }\nb.add(true)\nreturn 0 }",
        "@needs T: +(T) -> T\nstruct Box[T] { value: T }\nstruct Outer[U] { value: Box[U] }\nfn main() -> u8 { return 0 }",
        "@needs T: +(T) -> T\nstruct Box[T] { value: T }\nstruct Phantom[T] { value: i32 }\nfn main() -> u8 { fix x = Phantom[Box[bool]] { value: 1 }\nreturn 0 }",
    ] {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains("generic.needs-check"),
            "{error:?}"
        );
    }
}

#[test]
fn generic_structs_reject_invalid_layouts_receiver_parameters_and_variance() {
    for source in [
        "struct Bad[T] { value: Bad[T] }\nfn main() -> u8 { return 0 }",
        "struct Bad[T] { next: mut Bad[T] }\nfn main() -> u8 { return 0 }",
        "struct Grow[T] { next: mut? Grow[mut T] }\nfn main() -> u8 { return 0 }",
        "struct Box[T] { value: T }\nfn Box[U].get() -> U { return self.value }\nfn main() -> u8 { return 0 }",
        "struct Box[T] { value: T }\nfn read(box: Box[mut? i32]) -> void {}\nfn main() -> u8 { arena { fix p = new (1 as i32)\nread(Box[mut i32] { value: p }) }\nreturn 0 }",
        "struct Box[T] { value: T }\nfn main() -> u8 { fix box = Box { value: 1 }\nreturn 0 }",
    ] {
        assert!(suz::compile(source.as_bytes()).is_err(), "{source}");
    }
}

#[test]
fn generic_struct_field_stores_reject_shorter_arena_lifetimes() {
    let source = r#"
struct Box[T] { value: T }
fn main() -> u8 {
    arena {
        var saved = Box[mut? i32] { value: nil }
        arena {
            fix p = new (1 as i32)
            saved#value = p
        }
    }
    return 0
}
"#;
    let error = suz::compile(source.as_bytes()).unwrap_err();
    assert!(error.to_string().contains("arena.store"), "{error:?}");
}

#[test]
fn generic_structs_work_with_operators_and_concrete_function_shapes() {
    let output = compile_and_run_c23(
        r#"
struct Box[T] { value: T }
@needs T: +(T) -> T
fn Box[T].+(other: Box[T]) -> Box[T] { return Box[T] { value: self.value + other.value } }
@needs T: -() -> T
fn Box[T].-() -> Box[T] { return Box[T] { value: -self.value } }
struct Offset { value: i32 }
fn Offset.+(box: Box[i32]) -> i32 { return self.value + box.value }
fnshape Reader(Box[i32]) -> i32
fn read(box: Box[i32]) -> i32 { return box.value }
fn apply(f: fn Reader, box: Box[i32]) -> i32 { return f(box) }
@needs T: +(T) -> T
fn add[T](a: T, b: T) -> T { return a + b }
fn main() -> u8 {
    var box = Box[i32] { value: 3 }
    box += Box[i32] { value: 4 }
    fix sum = add[Box[i32]](box, box)
    if apply(fn read, sum) != 14 { return 1 }
    if (-box).value != -7 { return 2 }
    fix offset = Offset { value: 1 }
    if offset + box != 8 { return 3 }
    return 0
}
"#,
        "generic-struct-operators.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn generic_struct_methods_and_callbacks_preserve_pointer_lifetimes() {
    for invoke in [
        "saved#replace(p)",
        "apply(fn [var saved](p: mut? i32) -> void { saved#replace(p) }, p)",
    ] {
        let source = format!(
            r#"
struct Box[T] {{ value: T }}
fn Box[T]#replace(value: T) -> void {{ self#value = value }}
fnshape Update(mut? i32) -> void
fn apply(f: fn Update, p: mut? i32) -> void {{ f(p) }}
fn main() -> u8 {{
    arena {{
        var saved = Box[mut? i32] {{ value: nil }}
        arena {{
            fix p = new (1 as i32)
            {invoke}
        }}
    }}
    return 0
}}
"#
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains("arena.store"),
            "{invoke}: {error:?}"
        );
    }
}

#[test]
fn generic_structs_reject_collapsed_overloads_and_hidden_call_only_values() {
    for (source, rule) in [
        (
            "struct Box[T] { value: T }\nfn Box[T].+(rhs: T) -> bool { return true }\nfn Box[T].+(rhs: i32) -> bool { return true }\nfn main() -> u8 { fix b = Box[i8] { value: 1 }\nreturn 0 }",
            "operator.overload",
        ),
        (
            "struct Box[T] { value: T }\nfnshape Action() -> void\nfn bad(b: Box[fn Action]) -> void {}\nfn main() -> u8 { return 0 }",
            "generic.storage",
        ),
        (
            "struct Box[T] { value: T }\nfn unused[T](b: Box[T]) -> symbol { return b as symbol }\nfn main() -> u8 { return 0 }",
            "expr.as",
        ),
    ] {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(error.to_string().contains(rule), "{error:?}");
    }
}

#[test]
fn generic_struct_arguments_preserve_function_shape_identity() {
    let output = compile_and_run_c23(
        r#"
struct Box[T] { value: T }
fnshape First(i32) -> i32
fnshape Same(i32) -> i32
fnshape ReadFirst(Box[static fn First]) -> i32
fnshape ReadSame(Box[static fn Same]) -> i32
fn read(box: Box[static fn First]) -> i32 { return box.value(4) }
fn identity(n: i32) -> i32 { return n }
fn apply(f: fn ReadSame, box: Box[static fn Same]) -> i32 { return f(box) }
fn main() -> u8 {
    fix box = Box[static fn First] { value: fn identity }
    fix f: static fn ReadFirst = fn read
    if apply(f, box) != 4 { return 1 }
    return 0
}
"#,
        "generic-struct-fnshape-identity.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn generic_structs_parse_concrete_type_names_in_constants_and_runtime_values() {
    let output = compile_and_run_c23(
        r#"
struct Box[T] { value: T }
fix SAVED = "Box[i32] { value: 7 }" as Box[i32] or Box[i32] { value: 0 }
fn parse(source: symbol) -> Box[Box[i32]] {
    return source as Box[Box[i32]] or Box[Box[i32]] { value: SAVED }
}
fn main() -> u8 {
    if SAVED.value != 7 { return 1 }
    if parse("Box [ Box [ i32 ] ] { value: Box[i32] { value: 9 } }").value.value != 9 { return 2 }
    if parse("Box[Box[u32]] { value: Box[i32] { value: 9 } }").value.value != 7 { return 3 }
    return 0
}
"#,
        "generic-struct-text.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn generic_struct_equality_requires_equality_of_type_parameter_fields() {
    let output = compile_and_run_c23(
        r#"
struct Box[T] { value: T }
@needs T: ==(T) -> bool
fn same[T](a: Box[T], b: Box[T]) -> bool { return a == b }
@needs T: ==(T) -> bool
fn compare[T](a: T, b: T) -> bool { return a == b }
@needs T: ==(T) -> bool
fn forward[T](a: Box[T], b: Box[T]) -> bool { return compare[Box[T]](a, b) }
fn main() -> u8 {
    fix b = Box[i32] { value: 3 }
    if not same[i32](b, b) { return 1 }
    if not forward[i32](b, b) { return 2 }
    return 0
}
"#,
        "generic-struct-equality.suz",
    );
    assert!(output.status.success(), "{output:?}");
    for (source, rule) in [
        (
            "struct Box[T] { value: T }\nfn same[T](a: Box[T], b: Box[T]) -> bool { return a == b }\nfn main() -> u8 { return 0 }",
            "generic.operations",
        ),
        (
            "struct Box[T] { value: T }\nfn render[T](b: Box[T]) -> symbol { return \"value: ${b.value}\" }\nfn main() -> u8 { return 0 }",
            "symbol.interpolation",
        ),
    ] {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(error.to_string().contains(rule), "{error:?}");
    }
}

#[test]
fn generic_struct_recursion_checks_concrete_transitions() {
    for ty in ["i32", "mut i32"] {
        let source = format!(
            "struct Down[T] {{ next: mut? Down[i32] }}\nfn main() -> u8 {{ fix d = Down[{ty}] {{ next: nil }}\nreturn 0 }}"
        );
        suz::compile(source.as_bytes()).unwrap();
    }
    let source = "struct Down[T] { next: mut? Down[i32] }\nfn main() -> u8 { fix d = Down[u8] { next: nil }\nreturn 0 }";
    let error = suz::compile(source.as_bytes()).unwrap_err().to_string();
    assert!(error.contains("generic.recursion"), "{error}");
    assert!(error.contains("[u8] to [i32]"), "{error}");
}

#[test]
fn generic_struct_recursion_reports_declared_parameter_names() {
    for (source, expected) in [
        (
            "struct Grow[Item] { next: mut? Grow[mut Item] }",
            "[Item] to [mut Item]",
        ),
        (
            "struct Grow[Item] { next: mut? Grow[mut Item] }\nfn unused[Other, Value](v: Grow[Value]) -> void {}",
            "[Value] to [mut Value]",
        ),
        (
            "struct Box[T] { value: T }\nstruct Wrap[Item] { next: mut? Wrap[Box[Item]] }",
            "[Item] to [Box[Item]]",
        ),
        (
            "struct First[Item] { next: mut? Second[Item] }\nstruct Second[Other] { next: mut? First[mut Other] }",
            "[Item] to [mut Item]",
        ),
    ] {
        let source = format!("{source}\nfn main() -> u8 {{ return 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err().to_string();
        assert!(error.contains("generic.recursion"), "{error}");
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn generic_struct_recursion_reports_use_and_definition_locations() {
    for (usage, line, column) in [
        (
            "fn a(d: Down[u8]) -> void {}\nfn main() -> u8 { return 0 }",
            4,
            6,
        ),
        (
            "fn main() -> u8 {\n    fix x = Down[u8] { next: nil }\n    return 0\n}",
            5,
            13,
        ),
        (
            "fn a() -> Down[u8] { return Down[u8] { next: nil } }\nfn main() -> u8 { return 0 }",
            4,
            4,
        ),
        (
            "fn main() -> u8 {\n    fix x: Down[u8] = Down[u8] { next: nil }\n    return 0\n}",
            5,
            9,
        ),
    ] {
        let source = format!("struct Down[T] {{\n    next: mut? Down[i32],\n}}\n{usage}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "generic.check", "{error}");
        assert_eq!((error.line, error.column), (line, column), "{error}");
        assert!(error.message().contains("Down[u8]"), "{error}");
        assert_eq!(error.related.len(), 1, "{error}");
        let cause = &error.related[0];
        assert_eq!(cause.rule(), "generic.recursion", "{error}");
        assert_eq!((cause.line, cause.column), (2, 5), "{error}");
        assert!(cause.message().contains("[u8] to [i32]"), "{error}");
    }
}
