//! objectionの理由はソース上の注釈であり、式や文字列プールの値にはしない。
use super::*;

pub(super) fn scan(source: &str, line: usize, column: usize) -> Result<usize, Diagnostic> {
    let invalid = || {
        error(
            line,
            column,
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "arena.objection-reason",
                "expected a non-empty quoted reason with valid escapes and no control characters"
            ),
        )
    };
    let mut escaped = false;
    let mut nonblank = false;
    for (offset, ch) in source.char_indices().skip(1) {
        if ch.is_control() {
            return Err(invalid());
        }
        if escaped {
            if ch != '"' && ch != '\\' {
                return Err(invalid());
            }
            nonblank = true;
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return if nonblank {
                Ok(offset + 1)
            } else {
                Err(invalid())
            };
        } else {
            nonblank |= !ch.is_whitespace();
        }
    }
    Err(invalid())
}
