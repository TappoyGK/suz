use super::{CheckedStatement, check};
use crate::compile;
use crate::{
    ErrorKind,
    ast::{IntegerType, Type},
};

fn program(body: &str) -> String {
    format!("fn main() -> u8 {{\n{body}\n}}")
}

#[test]
fn infers_integer_literals_and_copies_without_back_propagating_return_type() {
    let source = program("fix first = 7\nfix second = (first)\nreturn 0");
    let tokens = crate::lexer::lex(&source).unwrap();
    let parsed = crate::parser::parse(&tokens).unwrap();
    let checked = check(&parsed).unwrap();
    for statement in &checked.functions[0].statements[..2] {
        let CheckedStatement::Binding { ty, .. } = statement else {
            panic!("expected a fix binding")
        };
        assert_eq!(*ty, Type::Integer(IntegerType::I64));
    }
    for body in [
        "fix value = 7\nreturn value",
        "fix first: i64 = 7\nfix second: u8 = first\nreturn 0",
    ] {
        let error = compile(program(body).as_bytes()).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidSource);
        assert!(error.message.contains("type.expected"), "{error}");
    }
}

#[test]
fn checks_each_integer_boundary_before_c_generation() {
    for (name, min, max) in crate::integer_bounds::INTEGER_BOUNDS {
        for value in [min, max] {
            compile(program(&format!("fix value: {name} = {value}\nreturn 0")).as_bytes()).unwrap();
        }
        for value in [min - 1, max + 1] {
            let error =
                compile(program(&format!("fix value: {name} = {value}\nreturn 0")).as_bytes())
                    .unwrap_err();
            assert!(error.message.contains("lit.range"), "{name}: {error}");
        }
    }
    let error = compile(
        program("fix value = 9999999999999999999999999999999999999999999999999\nreturn 0")
            .as_bytes(),
    )
    .unwrap_err();
    assert!(error.message.contains("lit.range"));
}

#[test]
fn checks_binding_scope_and_all_statements_including_unreachable_ones() {
    for (body, rule) in [
        ("fix value: u8 = value\nreturn value", "decl.local-scope"),
        (
            "fix first: u8 = later\nfix later: u8 = 7\nreturn first",
            "decl.local-scope",
        ),
        (
            "fix value: u8 = 1\nfix value: u8 = 2\nreturn value",
            "decl.duplicate",
        ),
        ("fix main: u8 = 7\nreturn 0", "decl.shadowing"),
        ("fix value: u8\nreturn 0", "var.initializer"),
        ("fix value: u8 = 7 return value", "src.statement-end"),
        ("fix value: u8 = 7", "fn.value-return-required"),
        ("return 7\nfix value: u8 = 256", "lit.range"),
        ("return 7\nreturn 256", "lit.range"),
        ("fix fn: u8 = 7\nreturn 0", "src.ident"),
    ] {
        let error = compile(program(body).as_bytes()).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidSource);
        assert!(error.message.contains(rule), "expected {rule}: {error}");
    }
    compile(program("return 7\nreturn 8").as_bytes()).unwrap();
}
#[test]
fn validates_mutability_assignment_types_and_reassignment_presence() {
    for (body, rule) in [
        (
            "fix value: u8 = 1\nvalue = 2\nreturn value",
            "var.fix-assign",
        ),
        ("var value: u8 = 1\nreturn value", "var.never-reassigned"),
        ("var value: u8\nvalue = 2\nreturn value", "var.initializer"),
        (
            "value = 2\nvar value: u8 = 1\nreturn value",
            "decl.local-scope",
        ),
        (
            "var value: u8 = value\nvalue = 2\nreturn value",
            "decl.local-scope",
        ),
        (
            "var value: u8 = 1\nfix wide: u16 = 2\nvalue = wide\nreturn value",
            "type.expected",
        ),
        ("var value: u8 = 1\nvalue = 256\nreturn value", "lit.range"),
        ("var value = 1\nvalue = 2\nreturn value", "type.expected"),
        (
            "var value: u8 = 1\nvalue = 2 value = 3\nreturn value",
            "src.statement-end",
        ),
        (
            "var value: u8 = 1\nfix value: u8 = 2\nreturn 0",
            "decl.duplicate",
        ),
        ("var main: u8 = 1\nmain = 2\nreturn 0", "decl.shadowing"),
        (
            "var value: u8 = 1\nfix copy = (value = 2)\nreturn copy",
            "assign.statement",
        ),
        (
            "var first: u8 = 1\nvar second: u8 = 2\nfirst = second = 3\nreturn first",
            "assign.statement",
        ),
    ] {
        let error = compile(program(body).as_bytes()).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidSource);
        assert!(error.message.contains(rule), "expected {rule}: {error}");
    }
    compile(program("var value: u8 = 1\nreturn value\nvalue = 2").as_bytes()).unwrap();
    compile(
        program("fix initial: u8 = 7\nvar value = initial\nvalue = value\nreturn value").as_bytes(),
    )
    .unwrap();
}

#[test]
fn diagnoses_negation_in_assignment_rhs() {
    let source = program("var value: i8 = 0\nvalue = -128\nreturn 0");
    let compiled =
        crate::compile_with_mode(source.as_bytes(), crate::ArithmeticMode::Warn).unwrap();
    assert_eq!(compiled.warnings.len(), 1);
    assert_eq!(
        (compiled.warnings[0].line, compiled.warnings[0].column),
        (3, 9)
    );
    let denied = crate::compile_with_mode(source.as_bytes(), crate::ArithmeticMode::Deny)
        .err()
        .unwrap();
    assert!(denied.message.contains("diagnostic.arithmetic"));
}
#[test]
fn block_scopes_reject_shadowing_and_expired_names() {
    for (body, rule) in [
        (
            "fix value: u8 = 1\n{ fix value: u8 = 2 }\nreturn 0",
            "decl.shadowing",
        ),
        ("{ fix main: u8 = 1 }\nreturn 0", "decl.shadowing"),
        ("{ fix value: u8 = 1 }\nreturn value", "decl.local-scope"),
        (
            "{ var value: u8 = 1\nvalue = 2 }\nvalue = 3\nreturn 0",
            "decl.local-scope",
        ),
        (
            "{ fix value: u8 = 1 }\n{ fix copy = value }\nreturn 0",
            "decl.local-scope",
        ),
        (
            "fix value: u8 = 1\n{ value = 2 }\nreturn value",
            "var.fix-assign",
        ),
        ("{ var value: u8 = 1 }\nreturn 0", "var.never-reassigned"),
        (
            "var outer: u8 = 1\n{ var inner: u8 = 2\nouter = inner }\nreturn outer",
            "var.never-reassigned",
        ),
        (
            "{ var value: u8 = 1 }\n{ var value: u8 = 2\nvalue = 3 }\nreturn 0",
            "var.never-reassigned",
        ),
        (
            "{ fix value: u8 = 1\nfix value: u8 = 2 }\nreturn 0",
            "decl.duplicate",
        ),
        ("{}return 0", "src.statement-end"),
        ("{} {}\nreturn 0", "src.statement-end"),
        ("{ fix value: u8 = 1 }", "fn.value-return-required"),
        ("return 0\n{ fix value: u8 = 256 }", "lit.range"),
    ] {
        let error = compile(program(body).as_bytes()).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidSource);
        assert!(error.message.contains(rule), "expected {rule}: {error}");
    }
    compile(program("{ fix value: u8 = 1 }\nfix value: u8 = 2\nreturn value").as_bytes()).unwrap();
    compile(program("var value: u8 = 1\nreturn value\n{ value = 2 }").as_bytes()).unwrap();
}

#[test]
fn nested_diagnostics_keep_physical_positions() {
    let source = program("{\n    { fix negative: i8 = -128 }\n}\nreturn 0");
    let compiled =
        crate::compile_with_mode(source.as_bytes(), crate::ArithmeticMode::Warn).unwrap();
    assert_eq!(compiled.warnings.len(), 1);
    assert_eq!(
        (compiled.warnings[0].line, compiled.warnings[0].column),
        (3, 26)
    );
}
