use super::check;

fn function_count(source: &str) -> usize {
    let tokens = crate::lexer::lex(source).unwrap();
    let parsed = crate::parser::parse(&tokens).unwrap();
    check(&parsed).unwrap().functions.len()
}

#[test]
fn emits_each_used_substitution_once_and_omits_unused_templates() {
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
fn nested_value_blocks_and_recovered_elif_branches_remain_bounded() {
    let mut value = "1".to_owned();
    for i in 0..20 {
        value = format!("expr {{ fix x{i} = {value}\nx{i} }}");
    }
    crate::compile(format!("fn main() -> u8 {{ fix n = {value}\nreturn 0 }}").as_bytes()).unwrap();
    let mut body = "return n".to_owned();
    for _ in 0..20 {
        body = format!("if n == 0 {{ {body} }} elif n < 1000 {{ return n }} else {{ return n }}");
    }
    crate::compile(
        format!("fn f(n: int) -> int {{ {body} }}\nfn main() -> u8 {{ return 0 }}").as_bytes(),
    )
    .unwrap();
}
