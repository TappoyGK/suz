//! 文字列テンプレートを文字列部分と補間式へ分け、エスケープと行の形式を検査する。
//!
//! 利用元: [`crate::lexer`]。
//! 主な対象規則: `[symbol.escape]`、`[symbol.interpolation]`、`[symbol.template]`、`[symbol.template-line]`。

use super::{Diagnostic, ErrorKind, error};

pub(crate) enum Segment<'a> {
    Text(&'a str),
    Expression { source: &'a str, column: usize },
}

pub(crate) fn scan(
    source: &str,
    line: usize,
    column: usize,
) -> Result<(usize, Vec<Segment<'_>>), Diagnostic> {
    let fail = |offset, message| {
        error(
            line,
            column + source[..offset].chars().count(),
            ErrorKind::InvalidSource,
            message,
        )
    };
    #[derive(Clone, Copy)]
    enum Mode {
        Text { start: usize, opening: usize },
        Expression { start: usize, braces: usize },
    }
    let bytes = source.as_bytes();
    let mut parts = Vec::new();
    // 字句解析は構文の深さが分かる前に行う。補間内のテンプレートも再帰せず走査する。
    let mut modes = vec![Mode::Text {
        start: 1,
        opening: 0,
    }];
    let mut cursor = 1;
    while cursor < bytes.len() {
        if bytes[cursor] == b'\n' {
            return Err(fail(
                cursor,
                crate::diagnostic!(
                    "symbol.template-line",
                    "template must fit on one source line"
                ),
            ));
        }
        match *modes.last().unwrap() {
            Mode::Text { start, .. } => match bytes[cursor] {
                b'"' => {
                    if modes.len() == 1 {
                        parts.push(Segment::Text(&source[start..cursor]));
                        return Ok((cursor + 1, parts));
                    }
                    modes.pop();
                    cursor += 1;
                }
                b'\\' => match bytes.get(cursor + 1) {
                    Some(b'0' | b'n' | b'r' | b't' | b'\\' | b'"' | b'$') => cursor += 2,
                    Some(b'\n') => {
                        return Err(fail(
                            cursor + 1,
                            crate::diagnostic!(
                                "symbol.template-line",
                                "template must fit on one source line"
                            ),
                        ));
                    }
                    _ => {
                        return Err(fail(
                            cursor,
                            crate::diagnostic!("symbol.escape", "unknown template escape"),
                        ));
                    }
                },
                b'$' if bytes.get(cursor + 1) == Some(&b'{') => {
                    if modes.len() == 1 {
                        parts.push(Segment::Text(&source[start..cursor]));
                    }
                    cursor += 2;
                    modes.push(Mode::Expression {
                        start: cursor,
                        braces: 1,
                    });
                }
                _ => cursor += 1,
            },
            Mode::Expression { start, braces } => match bytes[cursor] {
                ch if ch.is_ascii_alphabetic() || ch == b'_' => {
                    let word_start = cursor;
                    while bytes
                        .get(cursor)
                        .is_some_and(|ch| ch.is_ascii_alphanumeric() || *ch == b'_')
                    {
                        cursor += 1;
                    }
                    let word = &source[word_start..cursor];
                    if matches!(word, "objection" | "ignore") {
                        while matches!(bytes.get(cursor), Some(b' ' | b'\t')) {
                            cursor += 1;
                        }
                        if bytes.get(cursor) == Some(&b'[') {
                            // 理由中の括弧や引用符は補間式の構文として数えない。
                            let kind = if word == "ignore" {
                                super::reasons::Kind::Ignore
                            } else {
                                super::reasons::Kind::Objection
                            };
                            let reason_column = column + source[..cursor].chars().count();
                            cursor +=
                                super::reasons::scan(&source[cursor..], line, reason_column, kind)?;
                        }
                    }
                    if word == "allow" {
                        loop {
                            while matches!(bytes.get(cursor), Some(b' ' | b'\t')) {
                                cursor += 1;
                            }
                            if bytes.get(cursor) != Some(&b'[') {
                                break;
                            }
                            let item_column = column + source[..cursor].chars().count();
                            cursor += super::reasons::scan(
                                &source[cursor..],
                                line,
                                item_column,
                                super::reasons::Kind::Allow,
                            )?;
                            while matches!(bytes.get(cursor), Some(b' ' | b'\t')) {
                                cursor += 1;
                            }
                            if bytes.get(cursor) != Some(&b',') {
                                break;
                            }
                            cursor += 1;
                        }
                    }
                }
                b'\'' => {
                    let (_, length) = super::char_literal(&source[cursor..])
                        .map_err(|message| fail(cursor, message))?;
                    cursor += length;
                }
                b'"' => {
                    modes.push(Mode::Text {
                        start: cursor + 1,
                        opening: cursor,
                    });
                    cursor += 1;
                }
                b'/' if bytes.get(cursor + 1) == Some(&b'/') => {
                    if let Some(end) = source[cursor..].find('\n') {
                        return Err(fail(
                            cursor + end,
                            crate::diagnostic!(
                                "symbol.template-line",
                                "template must fit on one source line"
                            ),
                        ));
                    }
                    return Err(fail(
                        start,
                        crate::diagnostic!("symbol.interpolation", "unclosed interpolation"),
                    ));
                }
                b'{' => {
                    *modes.last_mut().unwrap() = Mode::Expression {
                        start,
                        braces: braces + 1,
                    };
                    cursor += 1;
                }
                b'}' => {
                    if braces == 1 {
                        modes.pop();
                        if modes.len() == 1 {
                            parts.push(Segment::Expression {
                                source: &source[start..cursor],
                                column: column + source[..start].chars().count(),
                            });
                        }
                        if let Mode::Text { start, .. } = modes.last_mut().unwrap() {
                            *start = cursor + 1;
                        }
                    } else {
                        *modes.last_mut().unwrap() = Mode::Expression {
                            start,
                            braces: braces - 1,
                        };
                    }
                    cursor += 1;
                }
                _ => cursor += 1,
            },
        }
    }
    match *modes.last().unwrap() {
        Mode::Text { opening, .. } => Err(fail(
            opening,
            crate::diagnostic!("symbol.template", "unclosed template"),
        )),
        Mode::Expression { start, .. } => Err(fail(
            start,
            crate::diagnostic!("symbol.interpolation", "unclosed interpolation"),
        )),
    }
}

pub(crate) fn decode(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        result.push(if ch == '\\' {
            match chars.next().unwrap() {
                '0' => '\0',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                ch @ ('\\' | '"' | '$') => ch,
                _ => unreachable!("template escapes are validated by the scanner"),
            }
        } else {
            ch
        });
    }
    result
}
