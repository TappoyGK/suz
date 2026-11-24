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
fn Number.copy() -> mut Number { return new self }
@needs T: .read() -> i32 or Failed
@needs T: #increment() -> void
fn use_value[T](value: T) -> i32 {
    var copy = value
    copy#increment()
    return copy.read() or 99
}
@needs T: .copy() -> mut T
fn duplicate[T](value: T) -> mut T { return value.copy() }
fn main() -> u8 {
    arena {
        fix p = duplicate[Number](Number { n: 4 })
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
        ("== (T) -> bool", "mut i32", "p"),
        ("*(T) -> f32", "i64", "1"),
    ] {
        let source = format!(
            "@needs T: {requirement}\nfn identity[T](value: T) -> T {{ return value }}\nfn main() -> u8 {{ arena {{ fix p = new (1 as i32)\nidentity[{ty}]({argument}) }}\nreturn 0 }}"
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
fn identity[T](value: T) -> T { return value }
fn main() -> u8 {
    arena {
        var escaped: mut? i32 = nil
        arena {
            fix p = new (1 as i32)
            escaped = identity[mut i32](p)
        }
    }
    return 0
}
"#;
    let error = suz::compile(source.as_bytes()).unwrap_err();
    assert!(error.message().contains("arena.store"), "{error:?}");
    let ordinary = b"fn identity[T](value: T) -> T { return value }\nfn main() -> u8 { arena { fix p = new (1 as i32)\nidentity[mut i32](p) }\nreturn 0 }";
    suz::compile(ordinary).unwrap();
    let recursive = r#"
fn grow[T]() -> void { grow[mut T]() }
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
        "fnshape Action() -> void\nfn local[T]() -> void { fix p: mut T = panic `stop` }\nfn main() -> u8 { local[fn Action]()\nreturn 0 }",
        "fnshape Action() -> void\nfn local[T]() -> void { fix p: T = panic `stop` }\nfn main() -> u8 { local[fn Action]()\nreturn 0 }",
        "fnshape Action() -> void\nfn exit_now[T]() -> T { return panic `stop` }\nfn main() -> u8 { exit_now[fn Action]()\nreturn 0 }",
        "fnshape Action() -> void\nfn unused[T](p: mut? T) -> void {}\nfn forward(f: fn Action) -> void { unused[fn Action](nil) }\nfn main() -> u8 { return 0 }",
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
        .replace("fn missing[U](a: U) -> U { return add.[U](a, a) }", "")
        .replace(
            "add[i32](1, 2)",
            "add[i32](1, 2)\nadd[i32](3, 4)\nadd[bool](true, false)",
        );
    let error = suz::compile(source.as_bytes()).unwrap_err();
    assert_eq!(error.rule(), "generic.needs-check");
}

#[test]
fn inferred_type_arguments_use_fixed_inputs_before_literal_defaults() {
    let output = compile_and_run_c23(
        r#"
fn first[T](a: T, b: T) -> T { return a }
fn identity[T](a: T) -> T { return a }
fn integer_default(a: i64) -> i64 { return a }
fn float_default(a: f64) -> f64 { return a }
fn main() -> u8 {
    fix small: i8 = 3
    fix real: f32 = 2.5
    if first(small, 4) != 3 { return 1 }
    if first(4, small) != 4 { return 2 }
    if first(real, 4.5) != 2.5 { return 3 }
    if first(4.5, real) != 4.5 { return 4 }
    if integer_default(identity(3)) != 3 { return 5 }
    if float_default(identity(2.5)) != 2.5 { return 6 }
    if identity(identity(small)) != 3 { return 7 }
    if first(1 + 2, small) != 3 { return 8 }
    return 0
}
"#,
        "generic-inference-literals.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn inferred_types_follow_nested_structures_pointers_and_forwarded_requirements() {
    let output = compile_and_run_c23(
        r#"
struct Box[T] { value: T }
fn unbox[T](box: Box[T]) -> T { return box.value }
fn read[T](p: mut T) -> T { return .p }
fn nilable[T](p: mut? T, value: T) -> T { let q = p { return .q }
return value }
@needs T: +(T) -> T
fn add[T](a: T, b: T) -> T { return a + b }
@needs Item: +(Item) -> Item
fn forward[Item](a: Item, b: Item) -> Item { return add(a, b) }
fn main() -> u8 {
    fix box = Box[Box[i32]] { value: Box[i32] { value: 9 } }
    if unbox(unbox(box)) != 9 { return 1 }
    fix n: i32 = 3
    if forward(n, 4) != 7 { return 2 }
    arena {
        fix p = new n
        if read(p) != 3 { return 3 }
        if nilable(nil, n) != 3 { return 4 }
        if nilable(p, n) != 3 { return 5 }
        if read(new 4) != 4 { return 6 }
    }
    return 0
}
"#,
        "generic-inference-structure.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn inference_conflicts_identify_both_arguments_without_widening() {
    for (a, b) in [("a", "b"), ("b", "a"), ("1", "1.0"), ("1.0", "1")] {
        let source = format!(
            "fn first[T](a: T, b: T) -> T {{ return a }}\nfn main() -> u8 {{\nfix a: i8 = 1\nfix b: i32 = 1\nfirst({a},\n{b})\nreturn 0\n}}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "generic.inference", "{error}");
        assert_eq!(error.line, 6, "{error}");
        assert_eq!(error.related.len(), 1, "{error}");
        assert_eq!(error.related[0].line, 5, "{error}");
        assert!(error.message().contains("conflicting types"), "{error}");
    }
}

#[test]
fn inference_conflicts_preserve_declared_names_in_nested_types() {
    let source = "struct Box[T] { value: T }\nfn first[T](a: T, b: T) -> T { return a }\nfn conflict[Left, Right](a: Box[Left], b: Box[Right]) -> void { first(a, b) }\nfn main() -> u8 { return 0 }";
    let error = suz::compile(source.as_bytes()).unwrap_err();
    assert_eq!(error.rule(), "generic.inference", "{error}");
    assert!(error.message().contains("Box[Left]"), "{error}");
    assert!(error.message().contains("Box[Right]"), "{error}");
}

#[test]
fn inference_requires_evidence_and_preserves_explicit_arguments() {
    for (definition, usage, rule) in [
        ("fn empty[T]() -> void {}", "empty()", "generic.inference"),
        (
            "fn identity[T](a: T) -> T { return a }",
            "fix n: mut? i32 = identity(nil)",
            "generic.inference",
        ),
        (
            "fn identity[T](a: T) -> T { return a }",
            "fix n: i8 = identity(1)",
            "type.expected",
        ),
        (
            "fn pair[T, U](a: T, b: U) -> T { return a }",
            "pair[i32](1, 2)",
            "generic.type-arguments",
        ),
        (
            "@needs T: +(T) -> T\nfn add[T](a: T, b: T) -> T { return a + b }",
            "add(true, false)",
            "generic.needs-check",
        ),
        (
            "fn first[T](a: T, b: T) -> T { return a }",
            "fix n: i8 = 1\nfirst(n, 128)",
            "lit.range",
        ),
    ] {
        let source = format!("{definition}\nfn main() -> u8 {{ {usage}\nreturn 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(error.to_string().contains(rule), "{source}\n{error}");
    }
    suz::compile(b"fn first[T](a: T, b: T) -> T { return a }\nfn main() -> u8 { fix a: i8 = 1\nfix b: i32 = 2\nfirst[i32](a, b)\nreturn 0 }").unwrap();
}

#[test]
fn inferred_calls_preserve_lifetimes_and_function_value_restrictions() {
    for source in [
        "fn identity[T](value: T) -> T { return value }\nfn main() -> u8 { arena { var saved: mut? i32 = nil\narena { fix p = new (1 as i32)\nsaved = identity(p) } }\nreturn 0 }",
        "fn identity[T](value: T) -> T { return value }\nfn main() -> u8 { identity(fn [](n: i32) -> i32 { return n })\nreturn 0 }",
        "fn nothing() -> void {}\nfn consume[T](v: T) -> void {}\nfn main() -> u8 { consume(nothing())\nreturn 0 }",
    ] {
        assert!(suz::compile(source.as_bytes()).is_err(), "{source}");
    }
    let output = compile_and_run_c23(
        r#"
fn consume[T](v: T) -> void {}
fn identity[T](v: T) -> T { return v }
fn plain(v: i32) -> i32 { return v }
fn main() -> u8 {
    consume(fn [](v: i32) -> i32 { return v })
    fix f = identity(fn plain)
    if f(3) != 3 { return 1 }
    return 0
}
"#,
        "inferred-function-values.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn inferred_named_calls_inside_numeric_generic_functions() {
    let output = compile_and_run_c23(
        r#"
fn identity[T](v: T) -> T { return v }
struct Box[T] { value: T }
fn pack[T](v: T) -> Box[T] { return Box[T] { value: v } }
fn unbox[T](v: Box[T]) -> T { return v.value }
@needs T: +(T) -> T
fn sum[T](a: T, b: T) -> T { return a + b }
fn numeric(v: int) -> int { return unbox(pack(identity(v))) }
fn real(v: float) -> float { return identity(v) }
fn doubled(v: int) -> int { return sum(v, v) }
fn main() -> u8 {
    fix a: i8 = 3
    fix b: f32 = 2.5
    if numeric(a) != 3 { return 1 }
    if real(b) != 2.5 { return 2 }
    if doubled(a) != 6 { return 3 }
    return 0
}
"#,
        "inferred-numeric-forward.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn inferred_numeric_conflicts_are_deferred_only_for_compatible_specializations() {
    let definition = "fn first[T](a: T, b: T) -> T { return a }\nfn numeric(a: int, b: i8) -> int { return first(a, b) }\n";
    suz::compile(format!("{definition}fn main() -> u8 {{ return 0 }}").as_bytes()).unwrap();
    suz::compile(format!("{definition}fn main() -> u8 {{ fix a: i8 = 1\nfix b: i8 = 2\nnumeric(a, b)\nreturn 0 }}").as_bytes()).unwrap();
    let source = format!(
        "{definition}fn main() -> u8 {{ fix a: i32 = 1\nfix b: i8 = 2\nnumeric(a, b)\nreturn 0 }}"
    );
    let error = suz::compile(source.as_bytes()).unwrap_err();
    assert!(error.to_string().contains("generic.inference"), "{error}");
}

#[test]
fn inference_preserves_branch_defaults_and_pointer_invariance() {
    let output = compile_and_run_c23(
        r#"
fn identity[T](v: T) -> T { return v }
fn main() -> u8 {
    fix a = identity(ifexpr true { 1 } else { 2.5 })
    fix b = identity(ifexpr true { 2.5 } else { 1 })
    if a != 1.0 { return 1 }
    if b != 2.5 { return 2 }
    return 0
}
"#,
        "inferred-branch-defaults.suz",
    );
    assert!(output.status.success(), "{output:?}");
    for source in [
        "struct Box[T] { value: T }\nfn take[T](b: Box[mut? T]) -> void {}\nfn main() -> u8 { arena { fix p = new (1 as i32)\ntake(Box[mut i32] { value: p }) }\nreturn 0 }",
        "fn same[T](a: T, b: T) -> void {}\nfn main() -> u8 { arena { fix p = new (1 as i32)\nfix q: mut? i32 = p\nsame(p, q) }\nreturn 0 }",
    ] {
        assert!(suz::compile(source.as_bytes()).is_err(), "{source}");
    }
}

#[test]
fn inferred_and_explicit_calls_generate_the_same_code() {
    let source = "fn first[T](a: T, b: T) -> T { return a }\nfn main() -> u8 { fix n: i32 = 3\nif first[i32](n, 4) != 3 { return 1 }\nreturn 0 }";
    // 引数の列位置も揃え、位置情報を含む生成Cそのものを比較する。
    let inferred = source.replace("first[i32]", "first     ");
    assert_eq!(
        suz::compile(source.as_bytes()).unwrap(),
        suz::compile(inferred.as_bytes()).unwrap()
    );
}

#[test]
fn inference_diagnostics_name_the_callee_type_parameters() {
    for (definition, usage, expected) in [
        (
            "fn first[T](a: T, b: T) -> T { return a }",
            "fix a: i16 = 1\nfix b: i32 = 2\nfirst(a, b)",
            "conflicting types i16 and i32 for type parameter T",
        ),
        (
            "fn choose[Key, Item](key: Key, a: Item, b: Item) -> Item { return a }\nfn forward[Left, Right](a: Left, b: Right) -> void { choose(true, a, b) }",
            "",
            "conflicting types Left and Right for type parameter Item",
        ),
        (
            "fn make[Result]() -> Result { panic `missing` }",
            "fix n: i32 = make()",
            "cannot infer type parameter Result; specify all type arguments explicitly",
        ),
        (
            "fn accept[Element](p: mut? Element) -> void {}",
            "accept(nil)",
            "cannot infer type parameter Element; specify all type arguments explicitly",
        ),
        (
            "fn pair[T, U](value: T) -> T { return value }",
            "pair(1)",
            "cannot infer type parameter U; specify all type arguments explicitly",
        ),
    ] {
        let source = format!("{definition}\nfn main() -> u8 {{ {usage}\nreturn 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "generic.inference", "{error}");
        assert!(error.message().contains(expected), "{error}");
    }
}
