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
    let next = match support::catalog::next_ids(&docs) {
        Ok(next) => next,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(prefix) = args.first() {
        match next.get(prefix) {
            Some(id) => println!("{id}"),
            None => {
                eprintln!("unknown prefix: {prefix}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        for id in next.values() {
            println!("{id}");
        }
    }
    ExitCode::SUCCESS
}
