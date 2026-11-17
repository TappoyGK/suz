//! CLIを起動し、診断・終了コード・シグナルをプロセスの終了へ反映する。内部パニックもCLIのエラーに変換する。
//!
//! OSから呼ばれる実行ファイルの入口。コマンドの処理は [`crate::cli`] に渡す。
//! 主な対象規則: `[tool.exit]`。

mod cli;

use std::{
    io::{self, Write},
    process::ExitCode,
};

fn main() -> ExitCode {
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(cli::run).unwrap_or_else(|_| {
        Err(cli::Failure::tool(
            3,
            "tool.exit",
            "internal compiler error",
        ))
    });
    match result {
        Ok(cli::Outcome::Code(code)) => ExitCode::from(code),
        Ok(cli::Outcome::Signal(number)) => cli::signals::terminate(number),
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "{}", error.message);
            ExitCode::from(error.code)
        }
    }
}
