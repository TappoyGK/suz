use super::{CheckedStatement, Value, check};

#[test]
fn float_literal_rounding_preserves_zero_sign_and_subnormals() {
    for (ty, literal, expected) in [
        ("f32", "-0.0", 0x8000_0000),
        ("f32", "-0", 0x8000_0000),
        ("f32", "-1.0e-50", 0x8000_0000),
        ("f32", "1.401298464324817070923729583289916131280e-45", 1),
        ("f32", "3.4028235e38", 0x7f7f_ffff),
        ("f32", "1.000000059604644775390625", 0x3f80_0000),
        (
            "f32",
            "1.0000000596046447753906250000000000000001",
            0x3f80_0001,
        ),
        ("f64", "-0.0", 0x8000_0000_0000_0000),
        ("f64", "-1.0e-400", 0x8000_0000_0000_0000),
        ("f64", "4.9406564584124654e-324", 1),
        (
            "f64",
            "1.00000000000000011102230246251565404236316680908203125",
            0x3ff0_0000_0000_0000,
        ),
    ] {
        let source = format!("fn main() -> u8 {{ fix value: {ty} = {literal}\nreturn 0 }}");
        let tokens = crate::lexer::lex(&source).unwrap();
        let program = check(&crate::parser::parse(&tokens).unwrap()).unwrap();
        let CheckedStatement::Binding { initializer, .. } = &program.functions[0].statements[0]
        else {
            panic!("expected binding")
        };
        let Value::Float(bits) = initializer.value else {
            panic!("expected float")
        };
        assert_eq!(bits, expected, "{ty}: {literal}");
        assert!(program.unhandled_arithmetic.is_empty());
    }
}
