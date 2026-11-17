//! Suzのソースを検査し、C23のソースへ変換するコンパイラ。
//!
//! [`compile_with_options`] と [`check_source_with_options`] が公開APIの入口となる。
//! 入力のUTF-8・改行（src.encoding、src.newline）を検査した後、次の順に処理する。
//!
//! 1. [`lexer`] がソースを位置付きの [`lexer::Token`] に分解する。
//! 2. [`parser`] がトークンから [`ast::Program`] を構築する。
//! 3. [`check`] が名前・型・値を検査し、[`check::ir::CheckedProgram`] を作る。
//! 4. [`warnings`] が診断候補へ警告設定とallowを適用する。
//! 5. [`codegen`] が検査済みIRからCを生成する。検査だけのAPIはこの処理を省く。
//!
//! モジュールと型の内部リンクをたどるには、非公開項目も含めてrustdocを生成する。

// 内部実装の案内も公開入口に載せるため、非公開項目を含むrustdoc生成を前提とする。
#![allow(rustdoc::private_intra_doc_links)]

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
    message: String,
    rule_id: String,
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
    pub fn message(&self) -> &str {
        &self.message
    }
    /// 表示用の文面から独立した規則識別子を返す。
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

/// 生成プログラムに埋め込む上限。Cランタイムでは0を無制限の指定に予約しているため、
/// 有限の上限として0は指定できない。
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

impl Warning {
    pub fn rule(&self) -> &str {
        &self.rule_id
    }
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CompilationOptions {
    pub log_allow: bool,
    pub deny_global_var: bool,
    pub arithmetic_mode: ArithmeticMode,
    pub text_pool_limit: TextPoolLimit,
}

#[derive(Debug)]
pub struct Warning {
    rule_id: String,
    pub allowed_reason: Option<String>,
    pub line: usize,
    pub column: usize,
    message: String,
}

pub struct Compilation {
    pub c: String,
    pub warnings: Vec<Warning>,
}

/// SuzのソースをC23へコンパイルする。
pub fn compile(bytes: &[u8]) -> Result<String, Diagnostic> {
    compile_with_mode(bytes, ArithmeticMode::Default).map(|result| result.c)
}

pub fn compile_with_mode(bytes: &[u8], mode: ArithmeticMode) -> Result<Compilation, Diagnostic> {
    compile_with_source_name(bytes, mode, "<input>")
}

/// 実行時の診断に使うソース名を指定してコンパイルする。
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

/// ランタイム設定を生成Cへ埋め込んでコンパイルする。
pub fn compile_with_options(
    bytes: &[u8],
    source_name: &str,
    options: &CompilationOptions,
) -> Result<Compilation, Diagnostic> {
    let (checked, warnings) = analyze(bytes, options)?;
    Ok(Compilation {
        c: codegen::generate(&checked, source_name, options.text_pool_limit),
        warnings,
    })
}

/// ソースを検査し、Cを生成せず警告を返す。
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
    analyze(bytes, options).map(|(_, warnings)| warnings)
}

fn analyze(
    bytes: &[u8],
    options: &CompilationOptions,
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
    // ASTは必要な名前や値を所有している。検査中にトークン列を保持しない。
    drop(tokens);
    let checked = check::check(&program)?;
    let warnings = warnings::collect(&checked, &program.allows, options)?;
    Ok((checked, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn statement_storage_stays_within_budget() {
        // メモリ効率を維持するため、全ての文の格納サイズが再び肥大化する変更を検出する。
        assert!(std::mem::size_of::<ast::Statement>() <= 200);
        assert!(std::mem::size_of::<check::ir::CheckedStatement>() <= 208);
    }

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
