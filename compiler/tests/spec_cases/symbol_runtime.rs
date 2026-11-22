use crate::test_util::TemporaryDirectory;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

// リリース検査では-O0と-O2の両方でサニタイザーを使う。
// 日常の検査は、サニタイザーの起動負荷なしで同じ検証を実行する。
fn sanitizer_flags(optimized: bool) -> &'static [&'static str] {
    if optimized || !cfg!(debug_assertions) {
        &["-fsanitize=address,undefined", "-fno-sanitize-recover=all"]
    } else {
        &[]
    }
}

fn executable(optimized: bool) -> &'static Path {
    static DEBUG: OnceLock<PathBuf> = OnceLock::new();
    static OPTIMIZED: OnceLock<PathBuf> = OnceLock::new();
    let slot = if optimized { &OPTIMIZED } else { &DEBUG };
    slot.get_or_init(|| {
        let directory = TemporaryDirectory::new();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let runtime = root.join("../runtime");
        let result = Command::new(std::env::var_os("CC").unwrap_or_else(|| "clang".into()))
            .args([
                "-std=c23",
                "-Wall",
                "-Wextra",
                "-Werror",
                if optimized { "-O2" } else { "-O0" },
            ])
            .args(sanitizer_flags(optimized))
            .arg("-I")
            .arg(&runtime)
            .arg(root.join("tests/spec_cases/symbol_runtime.c"))
            .arg(runtime.join("suz_parse.c"))
            .arg(runtime.join("suz_parse_float.c"))
            .arg(runtime.join("suz_runtime.c"))
            .args(["-lm", "-o"])
            .arg(directory.0.join("symbol-runtime"))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        // 全子プロセスで同じ実行ファイルのinodeを使う。実行ごとに書き直すとmacOSの実行ファイル検証が繰り返される。
        // この置き換え可能なビルド生成物は、Cargoの出力ディレクトリで管理する。
        let path = PathBuf::from(env!("OUT_DIR")).join(if optimized {
            "symbol-runtime-O2"
        } else {
            "symbol-runtime-O0"
        });
        let staging = path.with_extension(format!("{}.tmp", std::process::id()));
        fs::copy(directory.0.join("symbol-runtime"), &staging).unwrap();
        fs::rename(staging, &path).unwrap();
        path
    })
}

fn run(optimized: bool, mode: &str, value: &str, code: i32) -> std::process::Output {
    let output = Command::new(executable(optimized))
        .args([mode, value])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(code),
        "{mode}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    if code == 0 {
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    output
}

#[test]
fn interning_preserves_identity_in_both_registration_orders() {
    run(false, "identity", "0", 0);
}

#[test]
fn hash_collisions_preserve_distinct_contents() {
    run(false, "collision", "0", 0);
}

#[test]
fn pool_initialization_does_not_allocate_or_register_empty_symbol() {
    run(false, "empty", "1", 0);
}

#[test]
fn pool_accounts_dynamic_storage_and_metadata() {
    run(false, "accounting", "0", 0);
}

#[test]
fn existing_symbol_is_reused_at_the_pool_limit() {
    let probe = run(false, "reuse", "0", 0);
    let limit = String::from_utf8(probe.stdout).unwrap();
    run(false, "reuse", limit.trim(), 0);
}

#[test]
fn pool_limit_is_checked_before_allocation() {
    run(false, "limit", "1", 101);
}

#[test]
fn table_growth_accounts_both_allocations() {
    let probe = run(false, "growth", "0", 0);
    let peak: u64 = String::from_utf8(probe.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    run(false, "growth", &peak.to_string(), 0);
    let output = run(false, "growth", &(peak - 1).to_string(), 101);
    assert!(String::from_utf8_lossy(&output.stderr).contains("symbol pool limit exceeded"));
}

#[test]
fn temporary_buffers_do_not_accumulate_or_count_toward_the_pool() {
    run(false, "temporary", "0", 0);
}

#[test]
fn symbol_order_and_identity_preserve_unicode_and_nul() {
    run(false, "order", "0", 0);
}

#[test]
fn allocation_failures_preserve_accounting_and_do_not_allocate_for_diagnostics() {
    for call in ["1", "2", "3", "4"] {
        run(false, "oom", call, 101);
    }
    let output = run(false, "overflow", "0", 101);
    assert!(String::from_utf8_lossy(&output.stderr).contains("symbol length exceeds"));
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn optimized_symbol_pool_preserves_identity_accounting_and_failures() {
    for mode in [
        "identity",
        "collision",
        "empty",
        "accounting",
        "growth",
        "temporary",
        "order",
        "parse",
        "parse-float",
    ] {
        run(true, mode, "0", 0);
    }
    let probe = run(true, "reuse", "0", 0);
    let limit = String::from_utf8(probe.stdout).unwrap();
    run(true, "reuse", limit.trim(), 0);
    run(true, "limit", "1", 101);
    for call in ["1", "2", "3", "4"] {
        run(true, "oom", call, 101);
    }
    run(true, "overflow", "0", 101);
}

fn generated_with_environment(source: &str, driver: &str) -> std::process::Output {
    let (_directory, executable) = build_with_environment(source, driver);
    Command::new(executable).output().unwrap()
}

pub(super) fn build_with_environment(
    source: &str,
    driver: &str,
) -> (TemporaryDirectory, std::path::PathBuf) {
    build_with_environment_optimized(source, driver, false)
}

pub(super) fn build_with_environment_optimized(
    source: &str,
    driver: &str,
    optimized: bool,
) -> (TemporaryDirectory, std::path::PathBuf) {
    let directory = TemporaryDirectory::new();
    let mut c =
        suz::compile_with_source_name(source.as_bytes(), suz::ArithmeticMode::Default, "text.suz")
            .unwrap()
            .c;
    c = c.replace("int main(void)", "int suz_program(void)");
    c.push_str(include_str!("text_environment.c"));
    c.push_str(driver);
    let path = directory.0.join("program.c");
    let needs_arena = c.contains("#include \"suz_arena.h\"");
    let needs_float_parser = crate::support::float_runtime::parser_required(&c);
    let needs_struct_parser = crate::support::float_runtime::struct_parser_required(&c);
    let needs_float_format = crate::support::float_runtime::required(&c);
    fs::write(&path, c).unwrap();
    let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime");
    let executable = directory.0.join("program");
    let mut compiler = Command::new(std::env::var_os("CC").unwrap_or_else(|| "clang".into()));
    compiler
        .args(["-std=c23", "-Wall", "-Wextra", "-Werror"])
        .arg(if optimized { "-O2" } else { "-O0" })
        .args(sanitizer_flags(optimized))
        .arg("-I")
        .arg(&runtime)
        .arg(path)
        .arg(runtime.join("suz_runtime.c"))
        .args(needs_arena.then(|| runtime.join("suz_arena.c")))
        .arg(runtime.join("suz_text.c"))
        .arg(runtime.join("suz_parse.c"))
        .args(needs_float_parser.then(|| runtime.join("suz_parse_float.c")))
        .args(needs_struct_parser.then(|| runtime.join("suz_parse_struct.c")))
        .args(["-lm", "-o"])
        .arg(&executable);
    if needs_float_format {
        crate::support::float_runtime::link(&mut compiler, &runtime);
    }
    let output = compiler.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (directory, executable)
}

#[test]
fn log_fragments_do_not_allocate() {
    let result = generated_with_environment(
        r#"enum State: u8 { Ready }
fn main() -> u8 {
    for i in 0..<1000 {
        fix fraction = 1.5
        fix yes = true
        log info `value ${i} / ${fraction} / ${yes} / ${State.Ready}`
        log info ``
    }
    return 0
}
"#,
        r#"
int main(void) {
    text_fail_after = 0;
    assert(suz_program() == 0);
    assert(text_calls == 0 && text_live == 0 && text_peak == 0);
    assert(suz_symbol_pool_used() == 0);
    return 0;
}
"#,
    );
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let expected: String = (0..1000)
        .map(|i| format!("info text.suz:6: value {i} / 1.5 / true / Ready\ninfo text.suz:7: \n"))
        .collect();
    assert_eq!(result.stderr, expected.as_bytes());
}

#[test]
fn allocation_failures_report_the_operation_location() {
    for (source, line, column) in [
        (
            "fn main() -> u8 {\n    fix message: symbol = `constant`\n    return 0\n}",
            1,
            1,
        ),
        (
            "fn main() -> u8 {\n    fix message: symbol = `${ifexpr true { 7 } else { 8 }}`\n    return 0\n}",
            2,
            27,
        ),
    ] {
        let result = generated_with_environment(
            source,
            "int main(void) { text_fail_after = 0; return suz_program(); }\n",
        );
        assert_eq!(result.status.code(), Some(101));
        assert_eq!(
            String::from_utf8_lossy(&result.stderr),
            format!(
                "panic text.suz:{line}:{column}: symbol memory allocation failed [rt.symbol-allocation]\n"
            )
        );
    }
}

#[test]
fn logs_preserve_embedded_nul_bytes() {
    let source = "fn main() -> u8 {\n    fix message: symbol = `a\\0b`\n    log info `${message}`\n    return 0\n}";
    let result = generated_with_environment(source, "int main(void) { return suz_program(); }\n");
    assert_eq!(result.status.code(), Some(0));
    assert_eq!(result.stderr, b"info text.suz:3: a\0b\n");
}

#[test]
fn scalar_parsers_use_full_byte_slices_without_allocating() {
    run(false, "parse", "0", 0);
}

#[test]
fn float_parsers_preserve_rounding_without_allocations_or_terminators() {
    run(false, "parse-float", "0", 0);
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn float_parsers_ignore_numeric_locales() {
    let locales = Command::new("locale").arg("-a").output().unwrap();
    assert!(locales.status.success());
    for locale in String::from_utf8(locales.stdout)
        .unwrap()
        .lines()
        .filter(|name| *name == "C" || name.starts_with("de_DE") || name.starts_with("fr_FR"))
    {
        let output = Command::new(executable(true))
            .args(["parse-float", "0", locale])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{locale}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn scalar_count_limit_is_independent_of_unlimited_pool() {
    let output = run(false, "scalar-limit", "0", 101);
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("symbol length exceeds the supported range")
    );
}
