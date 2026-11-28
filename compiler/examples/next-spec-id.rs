// テスト基盤の catalog モジュールだけを共有するため、実行ランナー等の未使用項目を抑制する。
#[allow(dead_code, unused_imports)]
#[path = "../tests/support/mod.rs"]
mod support;

use std::{env, path::Path, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        eprintln!("usage: cargo run --quiet --example next-spec-id -- [PREFIX]");
        return ExitCode::SUCCESS;
    }
    if args.len() > 1 {
        eprintln!("expected at most one prefix argument");
        return ExitCode::FAILURE;
    }
    let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs");
    let ids = match support::catalog::next_ids(&docs, args.first().map(String::as_str)) {
        Ok(ids) => ids,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };
    for id in ids {
        println!("{id}");
    }
    ExitCode::SUCCESS
}
