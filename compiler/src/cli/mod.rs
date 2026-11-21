//! CLIの引数解析、ソース読み込み、検査・C出力・ビルド・実行をまとめる。ライブラリの診断を終了状態と表示へ変換する。
//!
//! 利用元: [`crate::main`]。
//! 主な対象規則: `[tool.build]`、`[tool.check]`、`[tool.emit-c]`、`[tool.exit]`、`[tool.information]`、`[tool.messages]`。

mod arguments;
mod assets;
mod emit;
mod foreign;
mod help;
mod native;
mod output;
pub mod signals;

use arguments::{Action, Command};
use std::{
    fs,
    io::{self, Write},
};

pub struct Failure {
    pub code: u8,
    pub message: String,
}
pub type Result<T> = std::result::Result<T, Failure>;

impl Failure {
    pub fn tool(code: u8, rule: &str, message: impl std::fmt::Display) -> Self {
        Self {
            code,
            message: format!("suz: {message} [{rule}]"),
        }
    }
}

fn display(text: &str) -> Result<()> {
    io::stdout()
        .lock()
        .write_all(text.as_bytes())
        .map_err(|e| Failure::tool(2, "tool.information", e))
}

pub enum Outcome {
    Code(u8),
    Signal(i32),
}

pub fn run() -> Result<Outcome> {
    match arguments::parse(std::env::args_os().skip(1))? {
        Action::Help(command) => display(&help::text(command)).map(|_| Outcome::Code(0)),
        Action::Version => display(concat!("suz v", env!("CARGO_PKG_VERSION"), "\n")).map(|_| Outcome::Code(0)),
        Action::Licenses => display(concat!(
            "Suz\nCopyright (c) 2026 TAPPOY G.K.\nLicensed under MIT OR Apache-2.0, at your option.\n\n",
            include_str!("../../../LICENSE-MIT"),
            "\n",
            include_str!("../../../LICENSE-APACHE"),
            "\n",
            "Ryu\nCopyright 2018 Ulf Adams\nLicensed under Boost Software License 1.0.\n\n",
            include_str!("../../../runtime/third_party/ryu/LICENSE-Boost"),
            "\n",
        )).map(|_| Outcome::Code(0)),
        Action::Execute(options) => {
            let build = if matches!(options.command, Command::Build | Command::Run) {
                Some(native::Build::prepare(&options)?)
            } else { None };
            let bytes = fs::read(&options.input).map_err(|e| {
                Failure::tool(
                    2,
                    if options.command == Command::Check {
                        "tool.check"
                    } else if options.command == Command::EmitC { "tool.emit-c" }
                    else { "tool.build" },
                    format!("{}: {e}", options.input.display()),
                )
            })?;
            let diagnostic = |error: suz::Diagnostic| match error.kind {
                suz::ErrorKind::InvalidSource => Failure {
                    code: 1,
                    message: format!("{}:{error}", options.input.display()),
                },
                suz::ErrorKind::Unsupported => Failure::tool(
                    3,
                    "tool.exit",
                    format!("internal error: {}:{error}", options.input.display()),
                ),
            };
            let compilation_options = suz::CompilationOptions { log_level: options.log_level, log_ignore: options.log_ignore, log_arena: options.log_arena, arithmetic_mode: options.mode, text_pool_limit: options.text_pool_limit, arena: options.arena, log_allow: options.log_allow };
            let (c, warnings) = if options.command == Command::Check {
                (
                    None,
                    suz::check_source_with_options(&bytes, &compilation_options).map_err(diagnostic)?,
                )
            } else {
                let compiled = suz::compile_with_options(
                    &bytes,
                    &options.input.display().to_string(),
                    &compilation_options,
                )
                .map_err(diagnostic)?;
                (Some(compiled.c), compiled.warnings)
            };
            // 注記を含む大量の警告を、書式の断片ごとのシステム呼び出しにしない。
            let mut stderr = io::BufWriter::new(io::stderr().lock());
            for warning in warnings {
                writeln!(
                    stderr,
                    "{}:{}:{}: {}: {}{}",
                    options.input.display(),
                    warning.line,
                    warning.column,
                    if warning.allowed_reason.is_some() { "allowed" } else { "warn" },
                    warning.message(),
                    warning.allowed_reason.as_ref().map(|reason| format!(" {reason}")).unwrap_or_default()
                )
                .map_err(|e| Failure::tool(2, "tool.messages", e))?;
                for note in &warning.related {
                    writeln!(stderr, "  note: {}:{}:{}: {}", options.input.display(), note.line, note.column, note.message())
                        .map_err(|e| Failure::tool(2, "tool.messages", e))?;
                }
            }
            stderr.flush().map_err(|e| Failure::tool(2, "tool.messages", e))?;
            drop(stderr);
            if let Some(c) = c {
                if let Some(build) = build {
                    let executable = build.compile(&options, &c)?;
                    if options.command == Command::Build {
                        build.publish(&options, &executable, options.output.as_deref().expect("validated output"))?;
                    } else {
                        let status = signals::execute(&mut std::process::Command::new(executable))
                            .map_err(|e| Failure::tool(2, "tool.run", e))?;
                        // mainが終了シグナルを再送する前に、ビルド用ディレクトリを破棄する。
                        drop(build);
                        use std::os::unix::process::ExitStatusExt;
                        return Ok(match status.signal() {
                            Some(number) => Outcome::Signal(number),
                            None => Outcome::Code(status.code().unwrap_or(1) as u8),
                        });
                    }
                } else {
                    let destination = options.output.as_deref().expect("validated output");
                    emit::emit(&c, destination, options.environment.as_deref(), &options.foreign)?;
                }
            }
            Ok(Outcome::Code(0))
        }
    }
}
