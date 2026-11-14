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
    let bytes = source.as_bytes();
    let mut parts = Vec::new();
    let mut cursor = 1;
    let mut text_start = cursor;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\n' => {
                return Err(fail(
                    cursor,
                    "template must fit on one source line [text.template-line]",
                ));
            }
            b'`' => {
                parts.push(Segment::Text(&source[text_start..cursor]));
                return Ok((cursor + 1, parts));
            }
            b'\\' => match bytes.get(cursor + 1) {
                Some(b'0' | b'n' | b'r' | b't' | b'\\' | b'`' | b'$') => cursor += 2,
                Some(b'\n') => {
                    return Err(fail(
                        cursor + 1,
                        "template must fit on one source line [text.template-line]",
                    ));
                }
                _ => return Err(fail(cursor, "unknown template escape [text.escape]")),
            },
            b'$' if bytes.get(cursor + 1) == Some(&b'{') => {
                parts.push(Segment::Text(&source[text_start..cursor]));
                cursor += 2;
                let start = cursor;
                let mut depth = 1;
                while cursor < bytes.len() {
                    match bytes[cursor] {
                        b'\n' => {
                            return Err(fail(
                                cursor,
                                "template must fit on one source line [text.template-line]",
                            ));
                        }
                        b'`' => {
                            let nested_column = column + source[..cursor].chars().count();
                            cursor += scan(&source[cursor..], line, nested_column)?.0;
                            continue;
                        }
                        b'/' if bytes.get(cursor + 1) == Some(&b'/') => {
                            if let Some(end) = source[cursor..].find('\n') {
                                return Err(fail(
                                    cursor + end,
                                    "template must fit on one source line [text.template-line]",
                                ));
                            }
                            return Err(fail(start, "unclosed interpolation [text.interpolation]"));
                        }
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    cursor += 1;
                }
                if depth != 0 {
                    return Err(fail(start, "unclosed interpolation [text.interpolation]"));
                }
                parts.push(Segment::Expression {
                    source: &source[start..cursor],
                    column: column + source[..start].chars().count(),
                });
                cursor += 1;
                text_start = cursor;
            }
            _ => cursor += 1,
        }
    }
    Err(fail(0, "unclosed template [text.template]"))
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
                ch @ ('\\' | '`' | '$') => ch,
                _ => unreachable!("template escapes are validated by the scanner"),
            }
        } else {
            ch
        });
    }
    result
}
