//! 宣言の種類に応じて名前の綴りを検査する。
//!
//! 利用元: [`crate::parser::functions`]、[`crate::parser::structs`]、[`crate::parser::enums`]。
//! 主な対象規則: `[name.top-level-binding]`、`[name.type]`、`[name.value]`、`[name.variant]`。

use crate::{Diagnostic, ErrorKind, ast::Location};

pub(super) enum Shape {
    TopLevelBinding,
    Value,
    Type,
    TypeParameter,
    Variant,
}

pub(super) fn check(shape: Shape, name: &str, location: Location) -> Result<(), Diagnostic> {
    let bytes = name.as_bytes();
    let first = bytes.first().copied().unwrap_or_default();
    let separated = !name.ends_with('_') && !name.contains("__");
    let (valid, rule, description) = match shape {
        Shape::TopLevelBinding => (
            first.is_ascii_uppercase()
                && bytes
                    .iter()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == b'_')
                && separated,
            "name.top-level-binding",
            "top-level binding names must start with an uppercase letter and contain only uppercase letters, digits, and single separating underscores",
        ),
        Shape::Value => (
            first.is_ascii_lowercase()
                && bytes
                    .iter()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
                && separated,
            "name.value",
            "function, variable, parameter, and field names must start with a lowercase letter and contain only lowercase letters, digits, and single separating underscores",
        ),
        Shape::Type => (
            first.is_ascii_uppercase()
                && bytes.iter().all(u8::is_ascii_alphanumeric)
                && bytes.iter().any(u8::is_ascii_lowercase)
                && !bytes
                    .windows(2)
                    .any(|pair| pair.iter().all(u8::is_ascii_uppercase)),
            "name.type",
            "type names must start with an uppercase letter, contain only letters and digits, include a lowercase letter, and have no adjacent uppercase letters",
        ),
        Shape::TypeParameter => (
            first.is_ascii_uppercase()
                && bytes.iter().all(u8::is_ascii_alphanumeric)
                && !bytes
                    .windows(2)
                    .any(|pair| pair.iter().all(u8::is_ascii_uppercase)),
            "name.type-parameter",
            "type parameter names must start with an uppercase letter, contain only letters and digits, and have no adjacent uppercase letters",
        ),
        Shape::Variant => (
            first.is_ascii_uppercase() && bytes.iter().all(u8::is_ascii_alphanumeric),
            "name.variant",
            "enum variant names must start with an uppercase letter and contain only letters and digits",
        ),
    };
    if valid {
        Ok(())
    } else {
        Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(rule, "invalid name '{name}': {description}"),
        ))
    }
}
