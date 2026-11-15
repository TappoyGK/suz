#[cfg(test)]
#[path = "../tests/support/integer_bounds.rs"]
mod integer_bounds;
mod numeric;

mod ast;
mod check;
mod codegen;
mod lexer;
mod parser;
mod warnings;

use std::{fmt, num::NonZeroU64};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    InvalidSource,
    Unsupported,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub related: Vec<Diagnostic>,
    pub kind: ErrorKind,
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub rule_id: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)?;
        for related in &self.related {
            write!(f, "\n  note: {related}")?;
        }
        Ok(())
    }
}

impl Diagnostic {
    pub fn new(
        kind: ErrorKind,
        line: usize,
        column: usize,
        rule: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        let rule_id = rule.into();
        Self {
            related: Vec::new(),
            kind,
            line,
            column,
            message: format!("{} [{}]", message.into(), rule_id),
            rule_id,
        }
    }
    /// The structured rule identifier, independent of the rendered message.
    pub fn rule(&self) -> &str {
        &self.rule_id
    }
}

#[derive(Clone)]
pub(crate) struct DiagnosticMessage {
    pub rule: &'static str,
    pub text: String,
}
macro_rules! diagnostic {
    ($rule:expr, $($message:tt)*) => {
        $crate::DiagnosticMessage { rule: $rule, text: format!($($message)*) }
    };
}
pub(crate) use diagnostic;

impl std::error::Error for Diagnostic {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ArithmeticMode {
    #[default]
    Default,
    Warn,
    Deny,
}

/// Limit embedded in generated programs. Zero is reserved by the C runtime
/// for unlimited storage and cannot be supplied as a finite limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextPoolLimit {
    Bytes(NonZeroU64),
    Unlimited,
}
impl Default for TextPoolLimit {
    fn default() -> Self {
        Self::Bytes(NonZeroU64::new(1_073_741_824).unwrap())
    }
}
impl TextPoolLimit {
    fn runtime_value(self) -> u64 {
        match self {
            Self::Bytes(bytes) => bytes.get(),
            Self::Unlimited => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CompilationOptions {
    pub log_allow: bool,
    pub arithmetic_mode: ArithmeticMode,
    pub text_pool_limit: TextPoolLimit,
}

#[derive(Debug)]
pub struct Warning {
    pub allowed_reason: Option<String>,
    pub line: usize,
    pub column: usize,
    pub message: String,
}

pub struct Compilation {
    pub c: String,
    pub warnings: Vec<Warning>,
}

/// Compile Suz source to C23.
pub fn compile(bytes: &[u8]) -> Result<String, Diagnostic> {
    compile_with_mode(bytes, ArithmeticMode::Default).map(|result| result.c)
}

pub fn compile_with_mode(bytes: &[u8], mode: ArithmeticMode) -> Result<Compilation, Diagnostic> {
    compile_with_source_name(bytes, mode, "<input>")
}

/// Compile with a source name used in generated runtime diagnostics.
pub fn compile_with_source_name(
    bytes: &[u8],
    mode: ArithmeticMode,
    source_name: &str,
) -> Result<Compilation, Diagnostic> {
    compile_with_options(
        bytes,
        source_name,
        &CompilationOptions {
            arithmetic_mode: mode,
            ..CompilationOptions::default()
        },
    )
}

/// Compile with runtime settings embedded in the generated C.
pub fn compile_with_options(
    bytes: &[u8],
    source_name: &str,
    options: &CompilationOptions,
) -> Result<Compilation, Diagnostic> {
    let (checked, warnings) = analyze(bytes, options.arithmetic_mode, options.log_allow)?;
    Ok(Compilation {
        c: codegen::generate(&checked, source_name, options.text_pool_limit),
        warnings,
    })
}

/// Validate source and return warnings without generating C.
pub fn check_source(bytes: &[u8], mode: ArithmeticMode) -> Result<Vec<Warning>, Diagnostic> {
    check_source_with_options(
        bytes,
        &CompilationOptions {
            arithmetic_mode: mode,
            ..CompilationOptions::default()
        },
    )
}

pub fn check_source_with_options(
    bytes: &[u8],
    options: &CompilationOptions,
) -> Result<Vec<Warning>, Diagnostic> {
    analyze(bytes, options.arithmetic_mode, options.log_allow).map(|(_, warnings)| warnings)
}

fn analyze(
    bytes: &[u8],
    mode: ArithmeticMode,
    log_allow: bool,
) -> Result<(check::CheckedProgram, Vec<Warning>), Diagnostic> {
    let source = std::str::from_utf8(bytes).map_err(|error| {
        let prefix = std::str::from_utf8(&bytes[..error.valid_up_to()]).unwrap();
        let (line, column) = lexer::position(prefix);
        Diagnostic::new(
            ErrorKind::InvalidSource,
            line,
            column,
            "src.encoding",
            "source is not valid UTF-8",
        )
    })?;
    for (offset, _) in source.match_indices('\r') {
        if source.as_bytes().get(offset + 1) != Some(&b'\n') {
            let (line, column) = lexer::position(&source[..offset]);
            return Err(Diagnostic::new(
                ErrorKind::InvalidSource,
                line,
                column,
                "src.newline",
                "CR must be followed by LF",
            ));
        }
    }
    let source = source.replace("\r\n", "\n");
    let tokens = lexer::lex(&source)?;
    let program = parser::parse(&tokens)?;
    let checked = check::check(&program)?;
    let warnings = warnings::collect(
        &checked.unhandled_arithmetic,
        &checked.comparison_warnings,
        &checked.division_warnings,
        &program.allows,
        mode,
        log_allow,
    )?;
    Ok((checked, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_comments_crlf_and_decimal_separators() {
        for source in [
            "// 前置き\nfn main() -> u8 { return 2_5_5 } // 後置き",
            "fn main(\r\n) -> u8 {\r\n\treturn 255\r\n}\r\n",
            "fn main() -> u8 { return 000255 }",
        ] {
            assert!(compile(source.as_bytes()).unwrap().contains("return 255;"));
        }
    }

    #[test]
    fn diagnoses_encoding_newlines_and_literal_bounds() {
        for (source, position, rule) in [
            (b"//\xff".as_slice(), (1, 3), "src.encoding"),
            (b"//\r".as_slice(), (1, 3), "src.newline"),
            (
                b"fn main() -> u8 {\n    return 256\n}".as_slice(),
                (2, 12),
                "lit.range",
            ),
            (
                b"fn main() -> u8 {\n    return 1__0\n}".as_slice(),
                (2, 12),
                "lit.integer",
            ),
            (
                b"fn main() -> u8 {\n    return 1_\n}".as_slice(),
                (2, 12),
                "lit.integer",
            ),
        ] {
            let error = compile(source).unwrap_err();
            assert_eq!(error.kind, ErrorKind::InvalidSource);
            assert_eq!((error.line, error.column), position);
            assert!(error.message.contains(rule), "{error}");
        }
    }

    #[test]
    fn validates_the_main_signature() {
        for (source, column) in [
            ("fn main() -> i32 { return 0 }", 14),
            ("fn main() -> void {}", 14),
            ("fn main(value: u8) -> u8 { return value }", 9),
            ("fn main() { return 0 }", 11),
            ("fn main -> u8 { return 0 }", 9),
        ] {
            let error = compile(source.as_bytes()).unwrap_err();
            assert_eq!(error.kind, ErrorKind::InvalidSource, "{source}");
            assert_eq!((error.line, error.column), (1, column), "{source}");
            assert!(error.message.contains("[prog.main]"), "{error}");
        }
        let error = compile(b"fn helper() -> u8 { return 0 }").unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidSource);
        assert!(error.message.contains("[prog.main]"));
    }

    #[test]
    fn rejects_return_without_a_value() {
        let error = compile(b"fn main() -> u8 { return\n7 }").unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidSource);
        assert!(error.message.contains("fn.return"));
    }

    #[test]
    fn does_not_accept_only_a_prefix_of_a_program() {
        for (source, kind) in [
            (
                "fn main() -> u8 { return 7 } fn other() -> void {}",
                ErrorKind::InvalidSource,
            ),
            (
                "fn main() -> u8 { return 7 } garbage",
                ErrorKind::InvalidSource,
            ),
        ] {
            assert_eq!(compile(source.as_bytes()).unwrap_err().kind, kind);
        }
    }
}
