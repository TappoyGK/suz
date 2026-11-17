pub use super::timing::print_timings;
use super::{
    Mode, Outcome, Run,
    timing::{C_TIME, RUN_TIME, SUZ_TIME, timed, timed_output},
};
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

type Result<T> = std::result::Result<T, String>;

pub fn worker_count(value: Option<&str>) -> Result<usize> {
    let Some(value) = value else {
        return Ok(8);
    };
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err("SUZ_JOBS must be a positive integer".into());
    }
    value
        .parse::<usize>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| "SUZ_JOBS must be a positive integer".into())
}

// 各ワーカーが次の未処理ジョブを取得する。完了順が変わっても結果は入力順に保ち、
// 通常の失敗で後続のジョブを取り消さない。
pub fn parallel_map<T: Sync, R: Send>(
    jobs: &[T],
    workers: usize,
    run: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    assert!(workers > 0, "worker count must be positive");
    let next = AtomicUsize::new(0);
    let mut results = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers.min(jobs.len()))
            .map(|_| {
                let (next, run) = (&next, &run);
                scope.spawn(move || {
                    let mut results = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(job) = jobs.get(index) else {
                            break;
                        };
                        results.push((index, run(job)));
                    }
                    results
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("case worker panicked"))
            .collect::<Vec<_>>()
    });
    results.sort_unstable_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}

pub fn build_and_run_c(c_file: &Path, executable: &Path) -> Result<Output> {
    let compiler = std::env::var_os("CC").unwrap_or_else(|| "clang".into());
    let mut command = Command::new(compiler);
    command
        .args(["-std=c23", "-Wall", "-Wextra", "-Werror"])
        .arg(c_file);
    let source =
        std::fs::read_to_string(c_file).map_err(|e| format!("could not read generated C: {e}"))?;
    if source.contains("#include \"suz_runtime.h\"") {
        let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime");
        command
            .arg("-I")
            .arg(&runtime)
            .arg(runtime.join("suz_runtime.c"))
            .arg(runtime.join("suz_text.c"))
            .arg(runtime.join("suz_parse.c"))
            .args(
                super::float_runtime::parser_required(&source)
                    .then(|| runtime.join("suz_parse_float.c")),
            )
            .arg("-lm")
            .arg(runtime.join("suz_env.c"));
        if super::float_runtime::struct_parser_required(&source) {
            command.arg(runtime.join("suz_parse_struct.c"));
        }
        if super::float_runtime::required(&source) {
            super::float_runtime::link(&mut command, &runtime);
        }
    }
    let built = timed_output(command.arg("-o").arg(executable), &C_TIME)
        .map_err(|e| format!("could not start the C compiler; check clang or CC: {e}"))?;
    if !built.status.success() {
        return Err(format!(
            "C compilation failed ({}): {}",
            built.status,
            String::from_utf8_lossy(&built.stderr)
        ));
    }
    timed_output(&mut Command::new(executable), &RUN_TIME)
        .map_err(|e| format!("could not start the generated program: {e}"))
}

pub fn execute(directory: &Path, id: &str, index: usize, run: &Run) -> Result<()> {
    let bytes = std::fs::read(directory.join(format!("{id}.suz")))
        .map_err(|e| format!("could not read source: {e}"))?;
    let Some(c_file) = compile(directory, id, index, run, &bytes)? else {
        return Ok(());
    };
    let output = build_and_run_c(&c_file, &directory.join(format!("{id}-{index}")))?;
    check_output(
        run,
        output.status.code(),
        &output.stdout,
        &output.stderr,
        &directory.join(format!("{id}.suz")),
    )
}

pub fn compile(
    directory: &Path,
    id: &str,
    index: usize,
    run: &Run,
    bytes: &[u8],
) -> Result<Option<PathBuf>> {
    if run.result == Outcome::Panic && run.panic_diagnostic.is_none() {
        return Err("missing panic diagnostic expectation".into());
    }
    if run.warnings.len() != run.warning_count {
        return Err("missing warning expectations".into());
    }
    if run.result == Outcome::CompileError && run.diagnostic.is_none() {
        return Err("missing diagnostic expectation".into());
    }
    let source = directory.join(format!("{id}.suz"));
    let c_file = directory.join(format!("{id}-{index}.c"));
    let mode = match run.mode {
        Mode::Default => suz::ArithmeticMode::Default,
        Mode::Warn => suz::ArithmeticMode::Warn,
        Mode::Deny => suz::ArithmeticMode::Deny,
    };
    let compiled = timed(&SUZ_TIME, || {
        suz::compile_with_source_name(bytes, mode, &source.display().to_string())
    });
    let compilation = match compiled {
        Err(error) => {
            if run.result != Outcome::CompileError {
                return Err(format!("Suz compilation failed: {error}"));
            }
            let expected = run.diagnostic.as_ref().unwrap();
            if error.kind != suz::ErrorKind::InvalidSource
                || !expected.matches_parts(error.line, error.column, error.message())
            {
                return Err(format!("expected {expected:?}, got {error:?}"));
            }
            if !run.warnings.is_empty() {
                return Err("expected warnings, but compilation failed without warnings".into());
            }
            return Ok(None);
        }
        Ok(compilation) => {
            if run.result == Outcome::CompileError {
                return Err("expected source error, but compilation succeeded".into());
            }
            compilation
        }
    };
    let mut remaining: Vec<_> = compilation.warnings.iter().collect();
    for expected in &run.warnings {
        let Some(index) = remaining.iter().position(|warning| {
            expected.matches_parts(warning.line, warning.column, warning.message())
        }) else {
            return Err(format!(
                "expected warnings {:?}, got {:?}",
                run.warnings, compilation.warnings
            ));
        };
        remaining.remove(index);
    }
    if !remaining.is_empty() {
        return Err(format!("unexpected warnings: {remaining:?}"));
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&c_file)
        .map_err(|e| format!("could not create generated C: {e}"))?;
    std::io::Write::write_all(&mut file, compilation.c.as_bytes())
        .map_err(|e| format!("could not write generated C: {e}"))?;
    Ok(Some(c_file))
}

pub fn check_output(
    run: &Run,
    code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
    source: &Path,
) -> Result<()> {
    if code != run.code.map(i32::from) {
        return Err(format!("expected exit code {:?}, got {code:?}", run.code));
    }
    if stdout != run.stdout.as_deref().unwrap_or_default().as_bytes() {
        return Err(format!(
            "stdout mismatch: expected {:?}, got {:?}",
            run.stdout,
            String::from_utf8_lossy(stdout)
        ));
    }
    if run.result == Outcome::Panic {
        let expected = run
            .panic_diagnostic
            .as_ref()
            .ok_or("missing panic diagnostic expectation")?;
        let prefix = run.stderr_prefix.as_deref().unwrap_or_default().as_bytes();
        let diagnostic = stderr
            .strip_prefix(prefix)
            .ok_or("panic stderr prefix mismatch")?;
        let diagnostic = std::str::from_utf8(diagnostic)
            .map_err(|e| format!("panic diagnostic is not UTF-8: {e}"))?;
        if !expected.matches_panic(diagnostic, source) {
            return Err(format!("expected panic {expected:?}, got {diagnostic:?}"));
        }
    } else if run.result == Outcome::Exit {
        if stderr != run.stderr.as_deref().unwrap_or_default().as_bytes() {
            return Err(format!(
                "stderr mismatch: expected {:?}, got {:?}",
                run.stderr,
                String::from_utf8_lossy(stderr)
            ));
        }
    } else {
        return Err("compile-error run has no runtime output".into());
    }
    Ok(())
}
