use crate::{Diagnostic, ErrorKind, ast::Location};

pub(super) enum Shape {
    Constant,
    Value,
    Type,
    Variant,
}

pub(super) fn check(shape: Shape, name: &str, location: Location) -> Result<(), Diagnostic> {
    let bytes = name.as_bytes();
    let first = bytes.first().copied().unwrap_or_default();
    let separated = !name.ends_with('_') && !name.contains("__");
    let (valid, rule, description) = match shape {
        Shape::Constant => (
            first.is_ascii_uppercase()
                && bytes
                    .iter()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == b'_')
                && separated,
            "name.constant",
            "constant names must start with an uppercase letter and contain only uppercase letters, digits, and single separating underscores",
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
            format!("invalid name '{name}': {description} [{rule}]"),
        ))
    }
}
