use crate::test_util::compile_and_run_c23;

#[test]
fn generic_function_values_and_shapes_use_concrete_signatures() {
    let source = r#"
fnshape Transform[T](T) -> T
fnshape Alias[U](U) -> U
struct Holder[T] { action: static fn Transform[T] }
fn identity[T](value: T) -> T { return value }
fn apply[T](action: fn Transform[T], value: T) -> T { return action(value) }
fn choose[T]() -> static fn Alias[T] { return fn identity[T] }
fix ID: static fn Transform[i32] = fn identity[i32]
fn main() -> u8 {
    fix f = fn identity[i32]
    fix holder = Holder[i32] { action: f }
    if f(4) != 4 || ID(5) != 5 || holder.action(6) != 6 { return 1 }
    if apply[i32](fn identity[i32], 7) != 7 { return 2 }
    if apply[i32](fn [](n: i32) -> i32 { return n + 1 }, 7) != 8 { return 3 }
    if choose[i32]()(8) != 8 || (fn identity[bool])(false) { return 4 }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "generic-function-values.suz");
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn generic_shapes_check_declared_requirements_before_erasing_names() {
    let prefix = "@needs T: +(T) -> T\nfnshape Add[T](T) -> T\nfnshape Plain[T](T) -> T\nfn identity[T](value: T) -> T { return value }\n";
    for body in [
        "fn main() -> u8 { fix f: static fn Add[bool] = fn identity[bool]\nreturn 0 }",
        "fn unused[T](f: static fn Add[T]) -> void {}\nfn main() -> u8 { return 0 }",
        "fnshape Outer[T](static fn Add[T]) -> void\nfn main() -> u8 { return 0 }",
        "struct Holder[T] { f: static fn Add[T] }\nfn main() -> u8 { return 0 }",
    ] {
        let error = suz::compile(format!("{prefix}{body}").as_bytes()).unwrap_err();
        assert!(error.to_string().contains("generic.needs-check"), "{error}");
    }
    let source = format!(
        "{prefix}@needs T: +(T) -> T\nfn forward[T](f: static fn Add[T], x: T) -> T {{ return f(x) }}\nfn main() -> u8 {{ fix plain: static fn Plain[bool] = fn identity[bool]\nfix add: static fn Add[i32] = fn identity[i32]\nif forward[i32](add, 4) != 4 || plain(false) {{ return 1 }}\nreturn 0 }}"
    );
    let output = compile_and_run_c23(&source, "generic-shape-needs.suz");
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn generic_function_values_require_complete_explicit_arguments() {
    let prefix = "fnshape Transform[T](T) -> T\nfn identity[T](x: T) -> T { return x }\nfn pair[T, U](x: T, y: U) -> T { return x }\nfn plain(x: i32) -> i32 { return x }\n";
    for statement in [
        "fix f = fn identity",
        "fix f: static fn Transform[i32] = fn identity",
        "fix f = fn identity[]",
        "fix f = fn identity[i32, bool]",
        "fix f = fn pair[i32]",
        "fix f = fn plain[i32]",
        "fix f: static fn Transform = fn identity[i32]",
        "fix f: static fn Transform[] = fn identity[i32]",
        "fix f: static fn Transform[bool] = fn identity[i32]",
    ] {
        let source = format!("{prefix}fn main() -> u8 {{ {statement}\nreturn 0 }}");
        assert!(suz::compile(source.as_bytes()).is_err(), "{source}");
    }
}

#[test]
fn generic_shapes_infer_arguments_and_preserve_nested_alias_identity() {
    let source = r#"
fnshape Transform[T](T) -> T
fnshape Alias[T](T) -> T
fnshape Higher[T](fn Transform[T], T) -> T
fnshape SameHigher[T](fn Alias[T], T) -> T
struct Box[T] { value: T }
fn identity[T](x: T) -> T { return x }
fn apply[T](f: fn Transform[T], x: T) -> T { return f(x) }
fn from_function[T](f: static fn Transform[T]) -> static fn Alias[T] { return f }
fn use_box[T](box: Box[static fn Alias[T]], x: T) -> T { return box.value(x) }
fn main() -> u8 {
    fix f = from_function(fn identity[i32])
    fix higher: static fn SameHigher[i32] = fn apply[i32]
    fix box = Box[static fn Transform[i32]] { value: f }
    if use_box(box, 5) != 5 || higher(fn identity[i32], 6) != 6 { return 1 }
    if apply(fn identity[i32], 7) != 7 { return 2 }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "generic-shape-inference.suz");
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn generic_shapes_allow_recursion_through_nominal_structs() {
    let source = r#"
fnshape Visit[T](Node[T]) -> T
fnshape Alias[T](Node[T]) -> T
struct Node[T] { value: T, visit: static fn Visit[T], alias: static fn Alias[T] }
fn accept[T](f: static fn Visit[T]) -> void {}
fn read[T](node: Node[T]) -> T { return node.value }
fn main() -> u8 {
    accept[i32](fn read[i32])
    fix node = Node[i32] { value: 7, visit: fn read[i32], alias: fn read[i32] }
    if node.visit(node) != 7 || node.alias(node) != 7 { return 1 }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "generic-shape-recursive.suz");
    assert!(output.status.success(), "{output:?}");
    for source in [
        "fnshape Bad[T](static fn Bad[T]) -> void",
        "fnshape First[T](static fn Second[T]) -> void\nfnshape Second[T](static fn First[T]) -> void",
    ] {
        assert!(
            suz::compile(format!("{source}\nfn main() -> u8 {{ return 0 }}").as_bytes()).is_err()
        );
    }
}

#[test]
fn generic_shapes_reuse_aliases_inside_recursive_type_arguments() {
    let source = r#"
struct Box[T] { value: T }
fnshape First[T](Box[static fn First[T]], T) -> T
fnshape Same[T](Box[static fn First[T]], T) -> T
fn accept[T](f: static fn First[T]) -> void {}
fn read[T](box: Box[static fn Same[T]], value: T) -> T { return value }
fn main() -> u8 {
    accept[i32](fn read[i32])
    fix first = Box[static fn First[i32]] { value: fn read[i32] }
    fix same: Box[static fn Same[i32]] = first
    if same.value(first, 4) != 4 { return 1 }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "generic-recursive-shape-arguments.suz");
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn generic_shapes_reject_growing_instantiation_through_struct_arguments() {
    let source = "struct Box[T] { value: T }\nfnshape Grow[T](Box[static fn Grow[ptr T]]) -> void\nfn main() -> u8 { return 0 }";
    let error = suz::compile(source.as_bytes()).unwrap_err();
    assert!(error.to_string().contains("generic.recursion"), "{error}");
}

#[test]
fn generic_function_values_keep_error_sets_and_storage_boundaries() {
    let source = r#"
error Failure { Bad, Other }
fnshape Action[T](T) -> T or Failure.Bad
fnshape Same[T](T) -> T or Failure.Bad
fn fail_value[T](x: T) -> T or Failure.Bad { fail Failure.Bad }
fn main() -> u8 {
    fix f: static fn Action[i32] = fn fail_value[i32]
    fix alias: static fn Same[i32] = f
    fix result = alias(1) or 9
    return ifexpr result == 9 { 0 } else { 1 }
}
"#;
    let output = compile_and_run_c23(source, "generic-function-errors.suz");
    assert!(output.status.success(), "{output:?}");
    let prefix = "fnshape Identity[T](T) -> T\nfn identity[T](x: T) -> T { return x }\n";
    for body in [
        "fn main() -> u8 { fix f: fn Identity[i32] = fn identity[i32]\nreturn 0 }",
        "fn bad[T](f: fn Identity[T]) -> static fn Identity[T] { return f }\nfn main() -> u8 { return 0 }",
        "fn bad[T](f: fn Identity[T]) -> void { fix saved = f }\nfn main() -> u8 { return 0 }",
        "arena fn allocate[T](x: T) -> ptr T { return new x }\nfn main() -> u8 { fix f = fn allocate[i32]\nreturn 0 }",
        "fn main() -> u8 { arena { var ptr saved: ptr? i32 = null\narena { fix ptr p = new (1 as i32)\nsaved = (fn identity[ptr i32])(p) } }\nreturn 0 }",
    ] {
        let source = format!("{prefix}{body}");
        assert!(suz::compile(source.as_bytes()).is_err(), "{source}");
    }
    assert!(
        suz::compile(
            source
                .replace("or Failure.Bad", "or Failure")
                .replacen(
                    "fnshape Same[T](T) -> T or Failure",
                    "fnshape Same[T](T) -> T or Failure.Bad",
                    1
                )
                .as_bytes()
        )
        .is_err()
    );
}

#[test]
fn generic_function_values_check_requirements_and_unused_bodies() {
    for source in [
        "@needs T: +(T) -> T\nfn add[T](x: T) -> T { return x + x }\nfn main() -> u8 { fix f = fn add[bool]\nreturn 0 }",
        "fn wide[T](x: T) -> u8 { return x as u8 }\nfn main() -> u8 { fix f = fn wide[i64]\nreturn 0 }",
    ] {
        assert!(suz::compile(source.as_bytes()).is_err(), "{source}");
    }
}

#[test]
fn generic_requirements_compare_function_shapes_after_resolving_aliases() {
    let source = r#"
fnshape First[T](T) -> T
fnshape Same[T](T) -> T
struct Worker { n: i32 }
fn Worker.use(f: static fn First[i32]) -> i32 { return f(self.n) }
fn identity[T](x: T) -> T { return x }
@needs T: .use(static fn First[U]) -> U
@needs T: .use(static fn Same[U]) -> U
fn invoke[T, U](x: T, f: static fn Same[U]) -> U { return x.use(f) }
@needs T: .use(static fn First[i32]) -> i32
struct Holder[T] { value: T }
@needs T: .use(static fn Same[i32]) -> i32
fn Holder[T].apply(f: static fn Same[i32]) -> i32 { return self.value.use(f) }
fn main() -> u8 {
    fix w = Worker { n: 7 }
    fix h = Holder[Worker] { value: w }
    if invoke(w, fn identity[i32]) != 7 || h.apply(fn identity[i32]) != 7 { return 1 }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "generic-shape-requirement-aliases.suz");
    assert!(output.status.success(), "{output:?}");
    let bad = source.replace("fnshape Same[T](T) -> T", "fnshape Same[T](T) -> bool");
    assert!(suz::compile(bad.as_bytes()).is_err());
}
