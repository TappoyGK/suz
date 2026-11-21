#[path = "../../tests/support/runtime_link.rs"]
mod runtime_link;
use super::{ExpressionContext, expressions::expression, generate};
use crate::ast::Type;
use crate::check::{CheckedExpression, CheckedProgram, CheckedStatement, Value};
use std::{fs, process::Command};

fn boolean(value: Value) -> CheckedExpression {
    CheckedExpression {
        ty: Type::Bool,
        value,
    }
}
fn not(value: Value) -> Value {
    Value::Not(Box::new(boolean(value)))
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn generated_bool_values_preserve_negation_and_copies() {
    // C生成を直接テストする。Suzにboolから整数への変換を追加せず、
    // Cの入口の終了コードとしてboolの値を観測する。
    let directory = std::env::temp_dir().join(format!("suz-bool-codegen-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    for (index, (value, expected)) in [
        (Value::Bool(false), 0),
        (Value::Bool(true), 1),
        (not(Value::Bool(false)), 1),
        (not(Value::Bool(true)), 0),
        (not(not(Value::Bool(true))), 1),
        (Value::Local(0), 0),
        (Value::Local(1), 1),
        (not(Value::Local(1)), 0),
    ]
    .into_iter()
    .enumerate()
    {
        let program = CheckedProgram {
            arrays: Default::default(),
            arena_warnings: Vec::new(),
            externs: Vec::new(),
            external_base: 0,
            function_types: Vec::new(),
            constant_texts: Default::default(),
            main_location: crate::ast::Location { line: 1, column: 1 },
            enums: Vec::new(),
            structs: Vec::new(),
            unhandled_arithmetic: Vec::new(),
            comparison_warnings: Vec::new(),
            division_warnings: Vec::new(),
            main_id: 0,
            functions: vec![crate::check::ir::CheckedFunction {
                lifetime: None,
                mutable_receiver: false,
                fallible: false,
                id: 0,
                parameters: Vec::new(),
                result: Type::Bool,
                statements: vec![
                    CheckedStatement::Binding {
                        location: crate::ast::Location { line: 1, column: 1 },
                        mutable: true,
                        id: 0,
                        ty: Type::Bool,
                        initializer: Box::new(boolean(Value::Bool(true))),
                    },
                    CheckedStatement::Binding {
                        location: crate::ast::Location { line: 1, column: 1 },
                        mutable: false,
                        id: 1,
                        ty: Type::Bool,
                        initializer: Box::new(boolean(Value::Local(0))),
                    },
                    CheckedStatement::Assign {
                        root_type: Type::Bool,
                        location: crate::ast::Location { line: 1, column: 1 },
                        fields: Vec::new(),
                        id: 0,
                        ty: Type::Bool,
                        value: Box::new(boolean(not(Value::Local(0)))),
                    },
                    CheckedStatement::Return {
                        value: boolean(value),
                        location: crate::ast::Location { line: 1, column: 1 },
                    },
                ],
            }],
        };
        let c_file = directory.join(format!("{index}.c"));
        let executable = directory.join(format!("{index}"));
        fs::write(
            &c_file,
            generate(
                &program,
                "test.suz",
                crate::TextPoolLimit::default(),
                crate::ArenaOptions::default(),
                crate::LogLevel::default(),
                false,
            ),
        )
        .unwrap();
        let output = runtime_link::compiler()
            .args(["-std=c23", "-Wall", "-Wextra", "-Werror"])
            .arg(&c_file)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result = Command::new(&executable).output().unwrap();
        assert_eq!(result.status.code(), Some(expected));
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
    }
}
#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn logical_codegen_evaluates_operands_once_and_short_circuits() {
    use crate::ast::LogicalOperator;
    let directory =
        std::env::temp_dir().join(format!("suz-logical-codegen-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    for (index, (operator, left, right, expected, trace)) in [
        (LogicalOperator::And, 0, 1, 0, 1),
        (LogicalOperator::And, 1, 0, 0, 12),
        (LogicalOperator::And, 1, 1, 1, 12),
        (LogicalOperator::Or, 1, 0, 1, 1),
        (LogicalOperator::Or, 0, 0, 0, 12),
        (LogicalOperator::Or, 0, 1, 1, 12),
    ]
    .into_iter()
    .enumerate()
    {
        let expr = boolean(Value::Logical {
            operator,
            left: Box::new(boolean(Value::Local(0))),
            right: Box::new(boolean(Value::Local(1))),
        });
        // Cのテスト用コードでローカルの読み取りを記録し、評価順と回数を確認する。
        // 評価を省略すべきオペランドも含める。
        let c = format!(
            r#"static int trace;
static _Bool probe(int id, _Bool value) {{ trace = trace * 10 + id; return value; }}
#define suz_local_0 probe(1, {left})
#define suz_local_1 probe(2, {right})
int main(void) {{
_Bool result = {};
return result != {expected} || trace != {trace};
}}
"#,
            expression(&expr, &mut ExpressionContext::default())
        );
        let source = directory.join(format!("{index}.c"));
        let executable = directory.join(format!("{index}"));
        fs::write(&source, c).unwrap();
        let output = runtime_link::compiler()
            .args(["-std=c23", "-Wall", "-Wextra", "-Werror"])
            .arg(&source)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result = Command::new(executable).output().unwrap();
        assert_eq!(result.status.code(), Some(0));
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
    }
}
#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn comparison_codegen_sequences_and_reuses_operands() {
    use crate::ast::{ComparisonOperator as Op, IntegerType, LogicalOperator};
    let directory = std::env::temp_dir().join(format!("suz-chain-codegen-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    for (index, (values, operators, expected, trace, skip)) in [
        ([1, 2, 3], vec![Op::Less, Op::Less], 1, 123, false),
        ([3, 2, 4], vec![Op::Less, Op::Less], 0, 12, false),
        ([1, 3, 2], vec![Op::Less, Op::Less], 0, 123, false),
        ([2, 2, 2], vec![Op::Equal, Op::Equal], 1, 123, false),
        (
            [3, 3, 1],
            vec![Op::GreaterEqual, Op::Greater],
            1,
            123,
            false,
        ),
        ([1, 1, 9], vec![Op::Equal], 1, 12, false),
        ([1, 2, 3], vec![Op::Less, Op::Less], 0, 0, true),
    ]
    .into_iter()
    .enumerate()
    {
        let operands = (0..operators.len() + 1)
            .map(|id| CheckedExpression {
                ty: Type::Integer(IntegerType::I64),
                value: Value::Local(id),
            })
            .collect();
        let mut expr = boolean(Value::Comparison {
            locations: Vec::new(),
            methods: Vec::new(),
            operators,
            operands,
        });
        if skip {
            expr = boolean(Value::Logical {
                operator: LogicalOperator::And,
                left: Box::new(boolean(Value::Bool(false))),
                right: Box::new(expr),
            });
        }
        let program = CheckedProgram {
            arrays: Default::default(),
            arena_warnings: Vec::new(),
            externs: Vec::new(),
            external_base: 0,
            function_types: Vec::new(),
            constant_texts: Default::default(),
            main_location: crate::ast::Location { line: 1, column: 1 },
            enums: Vec::new(),
            structs: Vec::new(),
            unhandled_arithmetic: Vec::new(),
            comparison_warnings: Vec::new(),
            division_warnings: Vec::new(),
            main_id: 0,
            functions: vec![crate::check::ir::CheckedFunction {
                lifetime: None,
                mutable_receiver: false,
                fallible: false,
                id: 0,
                parameters: Vec::new(),
                result: Type::Bool,
                statements: vec![CheckedStatement::Return {
                    value: expr,
                    location: crate::ast::Location { line: 1, column: 1 },
                }],
            }],
        };
        // ローカルの読み取りの記録は、C生成のテスト用コードだけに入れる。
        let c = format!(
            r#"static int trace;
static int probe(int id, int value) {{ trace = trace * 10 + id; return value; }}
#define suz_local_0 probe(1, {})
#define suz_local_1 probe(2, {})
#define suz_local_2 probe(3, {})
#define main generated_main
{}
#undef main
int main(void) {{
int result = generated_main();
return result != {expected} || trace != {trace};
}}
"#,
            values[0],
            values[1],
            values[2],
            generate(
                &program,
                "test.suz",
                crate::TextPoolLimit::default(),
                crate::ArenaOptions::default(),
                crate::LogLevel::default(),
                false
            )
        );
        let source = directory.join(format!("{index}.c"));
        let executable = directory.join(format!("{index}"));
        fs::write(&source, c).unwrap();
        let built = runtime_link::compiler()
            .args(["-std=c23", "-Wall", "-Wextra", "-Werror"])
            .arg(&source)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let result = Command::new(&executable).output().unwrap();
        assert_eq!(result.status.code(), Some(0));
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
    }
}

#[path = "tests/array_bounds.rs"]
mod array_bounds;
