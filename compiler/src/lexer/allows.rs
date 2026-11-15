use super::{Diagnostic, ErrorKind, error};

// An allow item is opaque to the ordinary lexer and template interpolation
// scanner: quotes, comment markers and braces in its reason are just text.
pub(crate) fn scan(source: &str, line: usize, column: usize) -> Result<usize, Diagnostic> {
    for (offset, ch) in source.char_indices().skip(1) {
        if ch == ']' {
            return Ok(offset + 1);
        }
        if ch <= '\u{1f}' || ch == '\u{7f}' {
            return Err(error(
                line,
                column + source[..offset].chars().count(),
                ErrorKind::InvalidSource,
                "control characters are forbidden in an allow item [allow.reason]",
            ));
        }
    }
    Err(error(
        line,
        column,
        ErrorKind::InvalidSource,
        "unclosed allow item [allow.item]",
    ))
}
