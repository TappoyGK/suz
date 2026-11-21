use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

pub(crate) struct TemporaryDirectory(pub(crate) PathBuf);
impl TemporaryDirectory {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "suz-cases-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("cannot create test directory: {e}"),
            }
        }
    }
}
impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn build_and_run_c(
    c_file: &std::path::Path,
    executable: &std::path::Path,
) -> std::process::Output {
    crate::support::runner::build_and_run_c(c_file, executable).unwrap()
}

// 生成コードをClangのC23モードで検査し、新しいプロセスで実行する。
pub(crate) fn compile_and_run_c23(source: &str, source_name: &str) -> std::process::Output {
    compile_and_run_c23_batch(&[(source, source_name)]).remove(0)
}

/// 同じCビルド条件の例をまとめてリンクする。構文検査とプロセスの分離は各例で維持する。
/// 外側のテストランナーと並列数を掛け合わせないよう、バッチ内のビルドは逐次実行する。
pub(crate) fn compile_and_run_c23_batch(sources: &[(&str, &str)]) -> Vec<std::process::Output> {
    let directory = TemporaryDirectory::new();
    let inputs = sources
        .iter()
        .enumerate()
        .map(|(index, (source, source_name))| {
            let path = directory.0.join(format!("case-{index}.c"));
            fs::write(
                &path,
                suz::compile_with_source_name(
                    source.as_bytes(),
                    suz::ArithmeticMode::Default,
                    source_name,
                )
                .unwrap_or_else(|error| panic!("case {index} ({source_name}): {error}"))
                .c,
            )
            .unwrap();
            let syntax = std::process::Command::new(
                std::env::var_os("CC").unwrap_or_else(|| "clang".into()),
            )
            .args([
                "-std=c23",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fsyntax-only",
                "-iquote",
            ])
            .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime"))
            .arg(&path)
            .output()
            .unwrap();
            assert!(
                syntax.status.success(),
                "case {index} ({source_name}): {}",
                String::from_utf8_lossy(&syntax.stderr)
            );
            (index, path)
        })
        .collect::<Vec<_>>();
    let batch = crate::support::batch::Batch::build(&directory.0, &inputs, 1).unwrap();
    sources
        .iter()
        .enumerate()
        .map(|(index, (_, name))| {
            batch
                .run(index)
                .unwrap_or_else(|error| panic!("case {index} ({name}): {error}"))
        })
        .collect()
}
