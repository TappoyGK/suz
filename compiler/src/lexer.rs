//! 改行を正規化したソースを、位置付きのトークンへ分解する。数値・文字・テンプレートの字句形式を検査する。
//!
//! 利用元: [`crate::analyze`]、[`crate::parser`]。
//! 主な対象規則: `[char.literal]`、`[for.range-spacing]`、`[lit.float]`、`[lit.integer]`、`[syntax.grammar]`。

use crate::{Diagnostic, ErrorKind};
mod reasons;
pub(crate) mod templates;
use std::ops::Range;

const KEYWORDS: &[&str] = &[
    "arena",
    "objection",
    "new",
    "ptr",
    "null",
    "let",
    "extern",
    "c_bytes",
    "c_byte_len",
    "unsafe",
    "allow",
    "fix",
    "var",
    "fn",
    "fnshape",
    "static",
    "struct",
    "enum",
    "error",
    "fail",
    "ignore",
    "recover",
    "if",
    "ifexpr",
    "elif",
    "else",
    "while",
    "loop",
    "for",
    "rev",
    "in",
    "break",
    "continue",
    "return",
    "match",
    "matchexpr",
    "as",
    "or",
    "not",
    "len",
    "log",
    "panic",
    "expr",
    "with",
    "true",
    "false",
    "nan",
    "inf",
];
const TYPES: &[&str] = &[
    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "len_t", "f32", "f64", "bool", "char",
    "text", "symbol", "void", "int", "float", "_int", "_uint", "_long", "_ulong", "_size_t",
    "_char",
];
const SYMBOLS: &[&str] = &[
    "(", ")", "[", "]", "{", "}", ",", ":", ".", "->", "=>", "=", "+=", "-=", "*=", "/=", "%=",
    "+%=", "-%=", "*%=", "&=", "|=", "^=", "<<=", ">>=", "+", "-", "*", "/", "%", "+%", "-%", "*%",
    "&", "|", "^", "~", "<<", ">>", "==", "!=", "<", "<=", ">", ">=", "&&", "||", "..<", "..=",
    "?", "#",
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Kind<'a> {
    Header(&'a str),
    ObjectionReason,
    IgnoreReason(&'a str),
    AllowItem(&'a str),
    Char(u32),
    Template(&'a str),
    Ident(&'a str),
    Keyword(&'a str),
    TypeName(&'a str),
    Integer(&'a str),
    Float(&'a str),
    Symbol(&'a str),
    Wildcard,
    Newline,
    End,
}

#[derive(Debug)]
pub(crate) struct Token<'a> {
    pub kind: Kind<'a>,
    pub line: usize,
    pub column: usize,
    /// CRLFを正規化した後のソース内のバイト範囲。
    pub span: Range<usize>,
}

pub(crate) fn position(prefix: &str) -> (usize, usize) {
    (
        prefix.bytes().filter(|b| *b == b'\n').count() + 1,
        prefix.rsplit('\n').next().unwrap().chars().count() + 1,
    )
}

fn error(
    line: usize,
    column: usize,
    kind: ErrorKind,
    message: crate::DiagnosticMessage,
) -> Diagnostic {
    Diagnostic::new(kind, line, column, message.rule, message.text)
}

fn valid_digit_separators(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.iter().enumerate().all(|(index, byte)| {
        *byte != b'_'
            || (index > 0
                && bytes[index - 1].is_ascii_digit()
                && bytes.get(index + 1).is_some_and(u8::is_ascii_digit))
    })
}

fn number(rest: &str) -> Result<(Kind<'_>, usize), crate::DiagnosticMessage> {
    let integer_end = rest
        .bytes()
        .take_while(|b| b.is_ascii_digit() || *b == b'_')
        .count();
    let mut end = integer_end;
    let bytes = rest.as_bytes();
    if bytes.get(end) == Some(&b'.')
        && bytes
            .get(end + 1)
            .is_some_and(|b| b.is_ascii_digit() || *b == b'_')
    {
        end += 1;
        end += bytes[end..]
            .iter()
            .take_while(|b| b.is_ascii_digit() || **b == b'_')
            .count();
        if bytes.get(end) == Some(&b'e') {
            end += 1;
            if matches!(bytes.get(end), Some(b'+' | b'-')) {
                end += 1;
            }
            let start = end;
            end += bytes[end..]
                .iter()
                .take_while(|b| b.is_ascii_digit() || **b == b'_')
                .count();
            if start == end {
                return Err(crate::diagnostic!(
                    "lit.float",
                    "a floating-point exponent requires digits"
                ));
            }
        }
        if !valid_digit_separators(&rest[..end]) {
            return Err(crate::diagnostic!(
                "lit.float",
                "a floating-point separator must be a single underscore between digits"
            ));
        }
        return Ok((Kind::Float(&rest[..end]), end));
    }
    let text = &rest[..end];
    if !valid_digit_separators(text) {
        return Err(crate::diagnostic!(
            "lit.integer",
            "an integer separator must be a single underscore between digits"
        ));
    }
    if bytes.get(end) == Some(&b'e') {
        return Err(crate::diagnostic!(
            "lit.float",
            "a floating-point exponent requires a decimal point"
        ));
    }
    // 範囲演算子と名前によるフィールド参照を、数値から分離する。
    if bytes.get(end) == Some(&b'.')
        && !bytes
            .get(end + 1)
            .is_some_and(|next| *next == b'.' || next.is_ascii_alphabetic() || *next == b'_')
    {
        return Err(crate::diagnostic!(
            "lit.float",
            "a decimal point requires digits on both sides"
        ));
    }
    Ok((Kind::Integer(text), end))
}

fn can_end_operand(kind: Option<Kind<'_>>) -> bool {
    matches!(
        kind,
        Some(
            Kind::Ident(_)
                | Kind::TypeName(_)
                | Kind::Integer(_)
                | Kind::Float(_)
                | Kind::Keyword("true" | "false" | "nan" | "inf")
                | Kind::Symbol(")" | "]" | "}")
        )
    )
}

fn continues_line(kind: Kind<'_>, previous: Option<Kind<'_>>) -> bool {
    match kind {
        Kind::Keyword("or") => true,
        Kind::Symbol("-") => can_end_operand(previous),
        Kind::Symbol(symbol) => matches!(
            symbol,
            "=" | "+="
                | "-="
                | "*="
                | "/="
                | "%="
                | "+%="
                | "-%="
                | "*%="
                | "&="
                | "|="
                | "^="
                | "<<="
                | ">>="
                | "+"
                | "*"
                | "/"
                | "%"
                | "+%"
                | "-%"
                | "*%"
                | "&"
                | "|"
                | "^"
                | "<<"
                | ">>"
                | "=="
                | "!="
                | "<"
                | "<="
                | ">"
                | ">="
                | "&&"
                | "||"
        ),
        _ => false,
    }
}

/// 入力の改行は正規化済みでなければならない。
pub(crate) fn lex(source: &str) -> Result<Vec<Token<'_>>, Diagnostic> {
    let mut tokens: Vec<Token<'_>> = Vec::new();
    let mut delimiters = Vec::new();
    let mut continuation = false;
    let (mut offset, mut line, mut column) = (0, 1, 1);
    while offset < source.len() {
        let rest = &source[offset..];
        let ch = rest.chars().next().unwrap();
        match ch {
            ' ' | '\t' => {
                offset += 1;
                column += 1;
                continue;
            }
            '/' if rest.starts_with("//") => {
                let length = rest.find('\n').unwrap_or(rest.len());
                column += rest[..length].chars().count();
                offset += length;
                continue;
            }
            '\n' => {
                if !matches!(delimiters.last(), Some(&"(" | &"[")) && !continuation {
                    tokens.push(Token {
                        kind: Kind::Newline,
                        line,
                        column,
                        span: offset..offset + 1,
                    });
                }
                offset += 1;
                line += 1;
                column = 1;
                continue;
            }
            '[' if matches!(tokens.last().map(|t| t.kind), Some(Kind::Keyword("ignore"))) => {
                let length = reasons::scan(rest, line, column, reasons::Kind::Ignore)?;
                tokens.push(Token {
                    kind: Kind::IgnoreReason(rest[1..length - 1].trim_matches(' ')),
                    line,
                    column,
                    span: offset..offset + length,
                });
                column += rest[..length].chars().count();
                offset += length;
                continuation = false;
                continue;
            }
            '[' if matches!(tokens.last().map(|t| t.kind), Some(Kind::Keyword("allow")))
                || (matches!(tokens.last().map(|t| t.kind), Some(Kind::Symbol(",")))
                    && matches!(
                        tokens.iter().rev().nth(1).map(|t| t.kind),
                        Some(Kind::AllowItem(_))
                    )) =>
            {
                let length = reasons::scan(rest, line, column, reasons::Kind::Allow)?;
                tokens.push(Token {
                    kind: Kind::AllowItem(&rest[1..length - 1]),
                    line,
                    column,
                    span: offset..offset + length,
                });
                offset += length;
                column += rest[..length].chars().count();
                continuation = false;
                continue;
            }
            '\'' => {
                let (value, length) = char_literal(rest)
                    .map_err(|message| error(line, column, ErrorKind::InvalidSource, message))?;
                tokens.push(Token {
                    kind: Kind::Char(value),
                    line,
                    column,
                    span: offset..offset + length,
                });
                offset += length;
                column += rest[..length].chars().count();
                continuation = false;
                continue;
            }
            '[' if tokens
                .last()
                .is_some_and(|t| t.kind == Kind::Keyword("objection")) =>
            {
                let length = reasons::scan(rest, line, column, reasons::Kind::Objection)?;
                tokens.push(Token {
                    kind: Kind::ObjectionReason,
                    line,
                    column,
                    span: offset..offset + length,
                });
                offset += length;
                column += rest[..length].chars().count();
                continuation = false;
                continue;
            }
            '"' if tokens
                .last()
                .is_some_and(|t| t.kind == Kind::Keyword("extern")) =>
            {
                let Some(end) = rest[1..].find('"') else {
                    return Err(error(
                        line,
                        column,
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("ffi.header", "unterminated header name"),
                    ));
                };
                let header = &rest[1..end + 1];
                if header.is_empty()
                    || !header
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_./-".contains(&b))
                    || header.starts_with('/')
                    || header
                        .split('/')
                        .any(|p| p.is_empty() || p == "." || p == "..")
                {
                    return Err(error(
                        line,
                        column,
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("ffi.header", "expected a relative C header name"),
                    ));
                }
                let length = end + 2;
                tokens.push(Token {
                    kind: Kind::Header(header),
                    line,
                    column,
                    span: offset..offset + length,
                });
                offset += length;
                column += length;
                continuation = false;
                continue;
            }
            '`' => {
                let length = templates::scan(rest, line, column)?.0;
                tokens.push(Token {
                    kind: Kind::Template(&rest[..length]),
                    line,
                    column,
                    span: offset..offset + length,
                });
                offset += length;
                column += rest[..length].chars().count();
                continuation = false;
                continue;
            }
            _ => {}
        }
        let (kind, length) = if ch.is_ascii_alphabetic() || ch == '_' {
            let length = rest
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
                .count();
            let word = &rest[..length];
            if word == "as" && rest.as_bytes().get(length) == Some(&b'%') {
                (Kind::Symbol("as%"), length + 1)
            } else {
                let kind = if word == "_" {
                    Kind::Wildcard
                } else if KEYWORDS.contains(&word) {
                    Kind::Keyword(word)
                } else if TYPES.contains(&word) {
                    Kind::TypeName(word)
                } else {
                    Kind::Ident(word)
                };
                (kind, length)
            }
        } else if ch.is_ascii_digit() {
            number(rest)
                .map_err(|message| error(line, column, ErrorKind::InvalidSource, message))?
        } else if ch == '.' && rest.as_bytes().get(1).is_some_and(u8::is_ascii_digit) {
            return Err(error(
                line,
                column,
                ErrorKind::InvalidSource,
                crate::diagnostic!("lit.float", "a decimal point requires digits on both sides"),
            ));
        } else if let Some(symbol) = SYMBOLS
            .iter()
            .filter(|symbol| rest.starts_with(**symbol))
            .max_by_key(|symbol| symbol.len())
        {
            (Kind::Symbol(symbol), symbol.len())
        } else {
            return Err(error(
                line,
                column,
                ErrorKind::InvalidSource,
                crate::diagnostic!("syntax.grammar", "unexpected character {ch:?}"),
            ));
        };
        continuation = continues_line(kind, tokens.last().map(|token| token.kind));
        match kind {
            Kind::Symbol("(" | "[" | "{") => {
                if let Kind::Symbol(symbol) = kind {
                    delimiters.push(symbol);
                }
            }
            Kind::Symbol("]") if delimiters.last() == Some(&"[") => {
                delimiters.pop();
            }
            Kind::Symbol(")") if delimiters.last() == Some(&"(") => {
                delimiters.pop();
            }
            Kind::Symbol("}") if delimiters.last() == Some(&"{") => {
                delimiters.pop();
            }
            _ => {}
        }
        tokens.push(Token {
            kind,
            line,
            column,
            span: offset..offset + length,
        });
        offset += length;
        column += length; // コメント以外の対応トークンはすべてASCII。
    }
    tokens.push(Token {
        kind: Kind::End,
        line,
        column,
        span: offset..offset,
    });
    for (index, token) in tokens.iter().enumerate() {
        if matches!(token.kind, Kind::Symbol("..<" | "..=")) {
            // 入力の端は空白ではない。単独で解析する補間式でも、
            // 境界値の欠落は構文解析側で構文エラーにする。
            let touches_left = if index == 0 {
                token.span.start == 0
            } else {
                !matches!(tokens[index - 1].kind, Kind::Newline | Kind::End)
                    && tokens[index - 1].span.end == token.span.start
            };
            let touches_right = tokens.get(index + 1).is_some_and(|next| {
                next.kind != Kind::Newline && next.span.start == token.span.end
            });
            if !touches_left || !touches_right {
                return Err(error(
                    token.line,
                    token.column,
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "for.range-spacing",
                        "range bounds must touch the range operator"
                    ),
                ));
            }
        }
    }
    Ok(tokens)
}

// リテラルの解析を、実行時の文字列から構造体への変換と共有する。
pub(crate) fn char_literal(input: &str) -> Result<(u32, usize), crate::DiagnosticMessage> {
    unsafe extern "C" {
        fn suz_parse_char_literal_status(
            bytes: *const u8,
            length: u64,
            output: *mut u32,
            consumed: *mut u64,
        ) -> u32;
    }
    let (mut value, mut consumed) = (0, 0);
    // 状態コードはsuz_char_literal_statusに従う。長さを限定したメモリ確保なしの呼び出し中、
    // 入力と出力は生存し続ける。
    let status = unsafe {
        suz_parse_char_literal_status(
            input.as_ptr(),
            input.len() as u64,
            &mut value,
            &mut consumed,
        )
    };
    match status {
        0 => Ok((value, consumed as usize)),
        2 => Err(crate::diagnostic!(
            "char.literal",
            "Unicode escape requires at least one hexadecimal digit"
        )),
        3 => Err(crate::diagnostic!(
            "char.literal",
            "Unicode escape accepts at most six hexadecimal digits"
        )),
        4 => Err(crate::diagnostic!(
            "char.literal",
            "Unicode escape cannot represent a surrogate (U+D800..U+DFFF)"
        )),
        5 => Err(crate::diagnostic!(
            "char.literal",
            "Unicode escape exceeds U+10FFFF"
        )),
        _ => Err(crate::diagnostic!(
            "char.literal",
            "expected one Unicode scalar in a character literal"
        )),
    }
}

/// 外部関数のSuz側の名前には、言語の予約語と組み込み型名を使わせない。
pub(crate) fn reserved(name: &str) -> bool {
    KEYWORDS.contains(&name) || TYPES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<Kind<'_>> {
        lex(source)
            .unwrap()
            .into_iter()
            .map(|token| token.kind)
            .collect()
    }

    #[test]
    fn malformed_floats_do_not_confuse_ranges_and_field_access() {
        for literal in ["1.", "1.)", "1.}", "1.\n", ".5", "1e3", "1e-3"] {
            let error = lex(&format!("// 日本語\n\t{literal}")).unwrap_err();
            assert_eq!(error.kind, ErrorKind::InvalidSource, "{literal}");
            assert_eq!((error.line, error.column), (2, 2), "{literal}");
            assert!(error.message.contains("[lit.float]"), "{literal}: {error}");
        }
        for source in ["1..<2", "1..=2", "1.value", "(1).value", "1.0", "1.0e-3"] {
            assert!(lex(source).is_ok(), "{source}");
        }
    }

    #[test]
    fn classifies_words_without_reserving_info() {
        assert_eq!(
            kinds("fn main u8 info _ _x fnx true nan inf"),
            vec![
                Kind::Keyword("fn"),
                Kind::Ident("main"),
                Kind::TypeName("u8"),
                Kind::Ident("info"),
                Kind::Wildcard,
                Kind::Ident("_x"),
                Kind::Ident("fnx"),
                Kind::Keyword("true"),
                Kind::Keyword("nan"),
                Kind::Keyword("inf"),
                Kind::End,
            ]
        );
    }

    #[test]
    fn takes_the_longest_operator_and_preserves_cast_boundaries() {
        assert_eq!(
            kinds("+%= +% += + <<= << <= < >>= >> >= > != == -> => as% as % aside%"),
            vec![
                Kind::Symbol("+%="),
                Kind::Symbol("+%"),
                Kind::Symbol("+="),
                Kind::Symbol("+"),
                Kind::Symbol("<<="),
                Kind::Symbol("<<"),
                Kind::Symbol("<="),
                Kind::Symbol("<"),
                Kind::Symbol(">>="),
                Kind::Symbol(">>"),
                Kind::Symbol(">="),
                Kind::Symbol(">"),
                Kind::Symbol("!="),
                Kind::Symbol("=="),
                Kind::Symbol("->"),
                Kind::Symbol("=>"),
                Kind::Symbol("as%"),
                Kind::Keyword("as"),
                Kind::Symbol("%"),
                Kind::Ident("aside"),
                Kind::Symbol("%"),
                Kind::End,
            ]
        );
    }

    #[test]
    fn reads_numbers_without_absorbing_ranges_or_unary_signs() {
        assert_eq!(
            kinds("-1_000 1.25e-3 0.0 12.5e+2 0..<10 1..=2"),
            vec![
                Kind::Symbol("-"),
                Kind::Integer("1_000"),
                Kind::Float("1.25e-3"),
                Kind::Float("0.0"),
                Kind::Float("12.5e+2"),
                Kind::Integer("0"),
                Kind::Symbol("..<"),
                Kind::Integer("10"),
                Kind::Integer("1"),
                Kind::Symbol("..="),
                Kind::Integer("2"),
                Kind::End,
            ]
        );
        for source in ["16_777_216.0", "0.123_456", "1.0e1_0", "1_2.3_4e-0_5"] {
            assert_eq!(kinds(source), vec![Kind::Float(source), Kind::End]);
        }
        for source in [
            "1__0", "1_", "1_.0", "1._0", "1__000.0", "1.0__0", "1.0_", "1.0e", "1.0e+", "1.0e_10",
            "1.0e+_10", "1.0e1__0", "1.0e10_",
        ] {
            assert_eq!(
                lex(source).unwrap_err().kind,
                ErrorKind::InvalidSource,
                "{source}"
            );
        }
    }

    #[test]
    fn continues_across_comments_and_blank_lines_after_operators() {
        assert_eq!(
            kinds("x = // comment\n\n 1 + // comment\n\n 2\ny or\n3"),
            vec![
                Kind::Ident("x"),
                Kind::Symbol("="),
                Kind::Integer("1"),
                Kind::Symbol("+"),
                Kind::Integer("2"),
                Kind::Newline,
                Kind::Ident("y"),
                Kind::Keyword("or"),
                Kind::Integer("3"),
                Kind::End,
            ]
        );
        assert_eq!(
            kinds("a\nor b"),
            vec![
                Kind::Ident("a"),
                Kind::Newline,
                Kind::Keyword("or"),
                Kind::Ident("b"),
                Kind::End
            ]
        );
        assert_eq!(
            kinds("(a\nor b)"),
            vec![
                Kind::Symbol("("),
                Kind::Ident("a"),
                Kind::Keyword("or"),
                Kind::Ident("b"),
                Kind::Symbol(")"),
                Kind::End
            ]
        );
    }

    #[test]
    fn innermost_braces_restore_statement_newlines() {
        assert_eq!(
            kinds("f(\nexpr {\nx\ny\n}\n)"),
            vec![
                Kind::Ident("f"),
                Kind::Symbol("("),
                Kind::Keyword("expr"),
                Kind::Symbol("{"),
                Kind::Newline,
                Kind::Ident("x"),
                Kind::Newline,
                Kind::Ident("y"),
                Kind::Newline,
                Kind::Symbol("}"),
                Kind::Symbol(")"),
                Kind::End,
            ]
        );
        assert_eq!(
            kinds("{(x\ny)\nz}"),
            vec![
                Kind::Symbol("{"),
                Kind::Symbol("("),
                Kind::Ident("x"),
                Kind::Ident("y"),
                Kind::Symbol(")"),
                Kind::Newline,
                Kind::Ident("z"),
                Kind::Symbol("}"),
                Kind::End,
            ]
        );
    }

    #[test]
    fn unary_operators_do_not_continue_lines_outside_parentheses() {
        assert_eq!(
            kinds("return -\n1"),
            vec![
                Kind::Keyword("return"),
                Kind::Symbol("-"),
                Kind::Newline,
                Kind::Integer("1"),
                Kind::End
            ]
        );
        assert_eq!(
            kinds("not\ntrue"),
            vec![
                Kind::Keyword("not"),
                Kind::Newline,
                Kind::Keyword("true"),
                Kind::End
            ]
        );
        assert_eq!(
            kinds("1 -\n2"),
            vec![
                Kind::Integer("1"),
                Kind::Symbol("-"),
                Kind::Integer("2"),
                Kind::End
            ]
        );
        assert_eq!(
            kinds("x as u8 -\n1"),
            vec![
                Kind::Ident("x"),
                Kind::Keyword("as"),
                Kind::TypeName("u8"),
                Kind::Symbol("-"),
                Kind::Integer("1"),
                Kind::End
            ]
        );
    }

    #[test]
    fn ranges_require_adjacent_bounds_even_across_suppressed_newlines() {
        for operator in ["..<", "..="] {
            for template in [
                "a ..<b",
                "a..< b",
                "a\n..<b",
                "a..<\nb",
                "(a\n..<b)",
                "(a..<\nb)",
                "a..<// comment\nb",
                " ..<b",
                "a..< ",
            ] {
                let source = template.replace("..<", operator);
                let error = lex(&source).unwrap_err();
                assert_eq!(error.kind, ErrorKind::InvalidSource, "{source}");
                assert!(error.message.contains("for.range-spacing"), "{source}");
            }
            for template in ["(a+1)..<(b+2)", "..<b", "a..<", "..<"] {
                let source = template.replace("..<", operator);
                assert!(lex(&source).is_ok(), "{source}");
            }
        }
    }

    #[test]
    fn keeps_physical_positions_when_newlines_are_suppressed() {
        let source = "a + // 日本語\n\t2";
        let tokens = lex(source).unwrap();
        let two = tokens
            .iter()
            .find(|token| token.kind == Kind::Integer("2"))
            .unwrap();
        assert_eq!((two.line, two.column), (2, 2));
        assert_eq!(&source[two.span.clone()], "2");
        assert_eq!(two.span.start, source.len() - 1);
        assert!(!tokens.iter().any(|token| token.kind == Kind::Newline));
    }

    #[test]
    fn distinguishes_invalid_characters_from_template_text() {
        for source in [";", "!", "\u{000b}", "名前"] {
            assert_eq!(lex(source).unwrap_err().kind, ErrorKind::InvalidSource);
        }
        assert_eq!(
            lex("log info `a // b`").unwrap()[2].kind,
            Kind::Template("`a // b`")
        );
    }
}
