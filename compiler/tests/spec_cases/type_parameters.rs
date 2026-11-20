use crate::test_util::compile_and_run_c23;

#[test]
fn explicit_type_arguments_preserve_integer_expression_boundaries() {
    let output = compile_and_run_c23(
        r#"
fn identity[T](value: T) -> T { return value }
@needs T: *(T) -> T
@needs T: +(T) -> T
fn multiply_add[T](a: T, b: T, c: T) -> T { return a * b + c }
@needs U: *(U) -> U
@needs U: +(U) -> U
fn forwarded[U](a: U, b: U, c: U) -> U { return multiply_add[U](a, b, c) }
fn main() -> u8 {
    if identity[i32](42) != 42 { return 1 }
    if not identity[bool](true) { return 2 }
    if forwarded[i8](100, 2, -100) != 100 { return 3 }
    if forwarded[f32](1.5, 2.0, 1.0) != 4.0 { return 4 }
    return 0
}
"#,
        "type-arguments.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn unused_generic_definitions_require_declared_operations() {
    for (declaration, rule) in [
        (
            "fn add[T](a: T, b: T) -> T { return a + b }",
            "generic.operations",
        ),
        (
            "@needs T: +(T) -> T\nfn add[T](a: T, b: T) -> T { return a + b or a }",
            "generic.arithmetic-handler",
        ),
        ("fn bad[T](a: T) -> bool { return a }", "type"),
        (
            "@needs T: +(T) -> T\nfn add[T](a: T, b: T) -> T { return a + b }\nfn forward[U](a: U, b: U) -> U { return add[U](a, b) }",
            "generic.needs-check",
        ),
    ] {
        let error =
            suz::compile(format!("{declaration}\nfn main() -> u8 {{ return 0 }}").as_bytes())
                .expect_err(declaration);
        assert!(error.message().contains(rule), "{declaration}: {error:?}");
    }
}

#[test]
fn generic_operator_and_method_requirements_use_declared_implementations() {
    let output = compile_and_run_c23(
        r#"
struct Number { value: i32 }
fn Number.+(other: Number) -> Number { return Number { value: self.value + other.value } }
fn Number.value_plus(n: i32) -> i32 { return self.value + n }
@needs T: +(T) -> T
fn sum[T](a: T, b: T) -> T { return a + b }
@needs T: .value_plus(i32) -> i32
fn read[T](value: T) -> i32 { return value.value_plus(2) }
fn main() -> u8 {
    fix n = sum[Number](Number { value: 2 }, Number { value: 3 })
    if read[Number](n) != 7 { return 1 }
    return 0
}
"#,
        "generic-methods.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn generic_requirements_keep_error_arena_and_mutability_contracts() {
    let output = compile_and_run_c23(
        r#"
error Failed { Bad }
struct Number { n: i32 }
fn Number.read() -> i32 or Failed { return self.n }
fn Number#increment() -> void { self#n = self.n + 1 }
arena fn Number.copy() -> ptr Number { return new self }
@needs T: .read() -> i32 or Failed
@needs T: #increment() -> void
fn use_value[T](value: T) -> i32 {
    var copy = value
    copy#increment()
    return copy.read() or 99
}
@needs T: arena .copy() -> ptr T
arena fn duplicate[T](value: T) -> ptr T { return value.copy() }
fn main() -> u8 {
    arena {
        fix ptr p = duplicate[Number](Number { n: 4 })
        if use_value[Number](.p) != 5 { return 1 }
    }
    return 0
}
"#,
        "generic-contracts.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn generic_calls_reject_unsatisfied_unused_requirements() {
    for (requirement, ty, argument) in [
        ("+(T) -> T", "bool", "true"),
        ("~() -> T", "len_t", "1"),
        ("== (T) -> bool", "ptr i32", "p"),
        ("*(T) -> f32", "i64", "1"),
    ] {
        let source = format!(
            "@needs T: {requirement}\nfn identity[T](value: T) -> T {{ return value }}\nfn main() -> u8 {{ arena {{ fix ptr p = new (1 as i32)\nidentity[{ty}]({argument}) }}\nreturn 0 }}"
        );
        let error = suz::compile(source.as_bytes()).expect_err(&source);
        assert!(
            error.message().contains("generic.needs-check"),
            "{source}: {error:?}"
        );
    }
}

#[test]
fn generic_requirement_lines_and_type_parameter_names_are_checked() {
    let valid = "@needs T: +(T) -> T // trailing\n// leading\nfn add[T](a: T, b: T) -> T { return a + b }\nfn main() -> u8 { return 0 }";
    suz::compile(valid.as_bytes()).unwrap();
    for declaration in [
        "struct Item { v: i32 }\nfn shadow[Item](v: Item) -> Item { return v }",
        "fn empty[](v: i32) -> i32 { return v }",
        "fn repeat[T, T](v: T) -> T { return v }",
        "fn capitals[TT](v: TT) -> TT { return v }",
        "fn lower[t](v: t) -> t { return v }",
        "fn mixed[T](v: T, n: int) -> T { return v }",
        "@needs T: +(T) -> T\n\nfn add[T](a: T, b: T) -> T { return a + b }",
        "@needs i32: +(i32) -> i32\nfn add[T](a: T) -> T { return a }",
        "@needs T: +(\nT) -> T\nfn add[T](a: T) -> T { return a }",
        "@needs T: +() -> T\nfn add[T](a: T) -> T { return a }",
        "@needs T: +(T) -> T\n@needs T: +(T) -> bool\nfn add[T](a: T) -> T { return a }",
    ] {
        let source = format!("{declaration}\nfn main() -> u8 {{ return 0 }}");
        assert!(suz::compile(source.as_bytes()).is_err(), "{source}");
    }
}

#[test]
fn named_generics_preserve_lifetimes_and_bound_recursive_instantiation() {
    let source = r#"
arena fn identity[T](value: T) -> T { return value }
fn main() -> u8 {
    arena {
        var ptr escaped: ptr? i32 = null
        arena {
            fix ptr p = new (1 as i32)
            escaped = identity[ptr i32](p)
        }
    }
    return 0
}
"#;
    let error = suz::compile(source.as_bytes()).unwrap_err();
    assert!(error.message().contains("arena.store"), "{error:?}");
    let ordinary = b"fn identity[T](value: T) -> T { return value }\nfn main() -> u8 { arena { fix ptr p = new (1 as i32)\nidentity[ptr i32](p) }\nreturn 0 }";
    let error = suz::compile(ordinary).unwrap_err();
    assert!(
        format!("{error:?}").contains("arena.parameter"),
        "{error:?}"
    );
    let recursive = r#"
fn grow[T]() -> void { grow[ptr T]() }
fn main() -> u8 { grow[i32]()
return 0 }
"#;
    let error = suz::compile(recursive.as_bytes()).unwrap_err();
    assert!(
        format!("{error:?}").contains("generic.recursion"),
        "{error:?}"
    );
    let finite = r#"
fn countdown[T](value: T, count: i32) -> T {
    if count == 0 { return value }
    return countdown[T](value, count - 1)
}
fn main() -> u8 { return countdown[u8](0, 3) }
"#;
    let output = compile_and_run_c23(finite, "generic-recursion.suz");
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn substituted_requirements_and_call_only_types_cannot_hide_invalid_signatures() {
    for source in [
        "@needs T: +(T) -> i8\n@needs U: +(U) -> i64\nfn keep[T, U](a: T, b: U) -> U { return b }\nfn main() -> u8 { keep[i8, i8](1, 2)\nreturn 0 }",
        "@needs T: .read() -> i32\n@needs T: arena .read() -> i32\nfn keep[T](v: T) -> T { return v }\nfn main() -> u8 { return 0 }",
        "fnshape Action() -> void\nfn local[T]() -> void { fix ptr p: ptr T = panic `stop` }\nfn main() -> u8 { local[fn Action]()\nreturn 0 }",
        "fnshape Action() -> void\nfn local[T]() -> void { fix p: T = panic `stop` }\nfn main() -> u8 { local[fn Action]()\nreturn 0 }",
        "fnshape Action() -> void\nfn exit_now[T]() -> T { return panic `stop` }\nfn main() -> u8 { exit_now[fn Action]()\nreturn 0 }",
        "fnshape Action() -> void\nfn unused[T](p: ptr? T) -> void {}\nfn forward(f: fn Action) -> void { unused[fn Action](null) }\nfn main() -> u8 { return 0 }",
    ] {
        assert!(suz::compile(source.as_bytes()).is_err(), "{source}");
    }
}

#[test]
fn requirements_accept_wider_integer_results_and_existing_numeric_methods() {
    let output = compile_and_run_c23(
        r#"
struct Number { n: i32 }
fn Number.*(k: int) -> Number { return Number { n: self.n * k } }
@needs T: *(i32) -> T
fn scale[T](a: T) -> T { return a * 2 }
@needs T: *(T) -> i64
fn wide[T](a: T, b: T) -> i64 { return a * b + 1 }
fn twice(n: int) -> int { return n * 2 }
fn call_numeric[T](a: T, n: i32) -> i32 { return twice(n) }
fn main() -> u8 {
    if scale[Number](Number { n: 3 }).n != 6 { return 1 }
    if wide[i8](100, 2) != 201 { return 2 }
    if call_numeric[bool](true, 2) != 4 { return 3 }
    return 0
}
"#,
        "generic-results.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn requirement_diagnostics_identify_error_sets_and_invalid_targets() {
    for (target, declaration, message) in [
        (
            "T",
            "fn main() -> u8 { return 0 }",
            "this declaration has no type parameters",
        ),
        (
            "T",
            "fn plain(a: i32) -> i32 { return a }\nfn main() -> u8 { return 0 }",
            "this declaration has no type parameters",
        ),
        (
            "U",
            "fn plain[T](a: T) -> T { return a }\nfn main() -> u8 { return 0 }",
            "'U' is not a type parameter of this declaration",
        ),
    ] {
        let source = format!("@needs {target}: +({target}) -> {target}\n{declaration}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "generic.needs");
        assert!(error.message().contains(message), "{error:?}");
    }
    for errors in ["Failed", "Failed.Bad", "Failed.Bad, Other"] {
        let source = format!(
            r#"
error Failed {{ Bad, Worse }}
error Other {{ Missing }}
@needs T: .get() -> i32 or {errors}
fn get[T](value: T) -> i32 {{ return value.get() or 0 }}
@needs U: .get() -> i32
fn forward[U](value: U) -> i32 {{ return get[U](value) }}
fn main() -> u8 {{ return 0 }}
"#
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "generic.needs-check");
        assert!(
            error
                .message()
                .contains(&format!(".get() -> i32 or {errors}")),
            "{error:?}"
        );
    }
}

#[test]
fn requirement_reuse_keeps_symbolic_callers_and_concrete_types_distinct() {
    let source = r#"
@needs T: +(T) -> T
fn add[T](a: T, b: T) -> T { return a + b }
@needs U: +(U) -> U
fn allowed[U](a: U) -> U { return add[U](a, a) }
fn missing[U](a: U) -> U { return add[U](a, a) }
fn main() -> u8 { add[i32](1, 2)
return 0 }
"#;
    let error = suz::compile(source.as_bytes()).unwrap_err();
    assert_eq!(error.rule(), "generic.needs-check");
    let source = source
        .replace("fn missing[U](a: U) -> U { return add[U](a, a) }", "")
        .replace(
            "add[i32](1, 2)",
            "add[i32](1, 2)\nadd[i32](3, 4)\nadd[bool](true, false)",
        );
    let error = suz::compile(source.as_bytes()).unwrap_err();
    assert_eq!(error.rule(), "generic.needs-check");
}
