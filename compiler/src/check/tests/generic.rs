use super::check;

fn function_count(source: &str) -> usize {
    let tokens = crate::lexer::lex(source).unwrap();
    let parsed = crate::parser::parse(&tokens).unwrap();
    check(&parsed).unwrap().functions.len()
}

#[test]
fn emits_each_used_substitution_once_and_omits_unused_templates() {
    // [runtime.num-monomorphize]: Cの書式に依存せず、検査済みの具体化の数を数える。
    let prefix = "fn id(n: int) -> int { return n }\nfn unused(n: int) -> int { return n }\n";
    assert_eq!(
        function_count(&format!("{prefix}fn main() -> u8 {{ return 0 }}")),
        1
    );
    assert_eq!(
        function_count(&format!(
            "{prefix}fn main() -> u8 {{ fix n: u8 = id(1)\nfix m: u8 = id(2)\nreturn 0 }}"
        )),
        2
    );
    assert_eq!(
        function_count(&format!(
            "{prefix}fn main() -> u8 {{ fix n: u8 = id(1)\nfix m: u16 = id(2)\nreturn 0 }}"
        )),
        3
    );
}

#[test]
fn nested_value_blocks_and_elif_branches_obey_a_traversal_budget() {
    // マシンの速度ではなく検査器の訪問回数を数える。
    // 指数的に処理が増える退行は、テストが停止しなくなる前に失敗させる。
    super::generic::with_work_budget(4000, || {
        let mut value = "1".to_owned();
        for i in 0..30 {
            value = format!("expr {{ fix x{i} = {value}\nx{i} }}");
        }
        crate::compile(format!("fn main() -> u8 {{ fix n = {value}\nreturn 0 }}").as_bytes())
            .unwrap();
        let mut body = "return n".to_owned();
        for _ in 0..30 {
            body =
                format!("if n == 0 {{ {body} }} elif n < 1000 {{ return n }} else {{ return n }}");
        }
        crate::compile(
            format!("fn f(n: int) -> int {{ {body} }}\nfn main() -> u8 {{ return 0 }}").as_bytes(),
        )
        .unwrap();
    });
}

#[test]
fn generic_body_is_checked_once_and_globals_are_shared() {
    // [num.fn-check]: 本体の走査は一度だけであり、8回や16回の具体型による再検査ではない。
    super::generic::with_work_budget(3, || {
        crate::compile(b"fn f(x: int, y: float) -> int { return x }\nfn main() -> u8 { return 0 }")
            .unwrap();
    });
    let scope = super::scope::Scope::default();
    let copy = scope.clone();
    assert!(std::rc::Rc::ptr_eq(&scope.constants, &copy.constants));
}

#[test]
fn generic_calls_inside_deep_value_blocks_are_checked_once() {
    // CLIのスタックはRustのテストワーカーより大きい。
    // 予算はスタックサイズやマシンの処理時間に依存せず、走査の作業量を測る。
    std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(|| {
        for generic_body in [false, true] {
            let mut value = "x +% 300".to_owned();
            // 呼び出し・expr・値ブロックの3段ずつを重ね、構文の上限256以内にする。
            for i in 0..80 {
                value = format!("f(expr {{ fix t_{i} = {value}\nt_{i} }})");
            }
            let source = if generic_body {
                format!("fn f(y: int) -> int {{ return y }}\nfn g(x: int) -> int {{ fix r = {value}\nreturn x }}\nfn main() -> u8 {{ fix a: i64 = 1\nfix q = g(a)\nreturn 0 }}")
            } else {
                format!("fn f(y: int) -> int {{ return y }}\nfn main() -> u8 {{ fix x: i64 = 1\nfix r = {value}\nreturn 0 }}")
            };
            super::generic::with_work_budget(5_000, || {
                let tokens = crate::lexer::lex(&source).unwrap();
                let parsed = crate::parser::parse(&tokens).unwrap();
                check(&parsed).unwrap();
            });
        }
    }).unwrap().join().unwrap();
}

#[test]
fn named_function_values_reuse_direct_call_instances() {
    let prefix = "fn identity[T](x: T) -> T { return x }\nfn unused[T](x: T) -> T { return x }\n";
    assert_eq!(
        function_count(&format!(
            "{prefix}fn main() -> u8 {{ fix x = identity[i32](1)\nfix f = fn identity[i32]\nfix g = fn identity[i32]\nreturn 0 }}"
        )),
        2
    );
    assert_eq!(
        function_count(&format!(
            "{prefix}fn main() -> u8 {{ fix f = fn identity[i32]\nfix g = fn identity[bool]\nreturn 0 }}"
        )),
        3
    );
}

#[test]
fn unrelated_range_declaration_does_not_change_generic_check() {
    let base = "fn zero() -> int {\n    return 0\n}\nfn main() -> u8 {\n    return 0\n}\n";
    let with_range = format!("range Divisor: u16 = 1..=1000\n{base}");
    let (_, w1) = crate::analyze(base.as_bytes(), &crate::CompilationOptions::default()).unwrap();
    let (_, w2) =
        crate::analyze(with_range.as_bytes(), &crate::CompilationOptions::default()).unwrap();
    assert_eq!(w1.len(), w2.len());
}
