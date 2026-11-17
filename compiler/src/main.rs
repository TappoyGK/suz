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
    // 深さ256の解析・検査・C生成と途中の木の破棄に必要なスタックを確保する。
    // OSのメインスレッドのスタックサイズに、言語の受理範囲を依存させない。
    let result = std::panic::catch_unwind(|| {
        let worker = std::thread::Builder::new()
            .name("suz-compiler".into())
            .stack_size(64 * 1024 * 1024)
            .spawn(cli::run)
            .expect("cannot create compiler thread");
        worker
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
    .unwrap_or_else(|_| {
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
