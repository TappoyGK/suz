//! 検査を故意に外したIRも生成し、ASanが範囲外アクセスを実際に検出できるか確かめる。
use super::*;

fn omit_bounds(value: &mut CheckedExpression) {
    match &mut value.value {
        Value::ArrayElement { indices, .. } => {
            for (_, _, proven) in indices {
                *proven = true;
            }
        }
        Value::ArrayView { proven, .. } => *proven = true,
        _ => {}
    }
    value
        .value
        .visit_constant_parts_mut(&mut omit_bounds, &mut |_| {});
}

#[test]
#[ignore = "release check: array ASan negative control"]
fn omitted_array_bounds_are_detected_by_asan() {
    let directory = std::env::temp_dir().join(format!("suz-array-negative-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    for (index, (ty, init, access)) in [
        ("#[i64]", "new #[1]", "a.[i]"),
        ("#[i64]", "new #[1]", "a.[0:end].[i]"),
        ("#[i64; _, _]", "new #[1; 1, 1]", "a.[i].[:].[0]"),
    ]
    .into_iter()
    .enumerate()
    {
        let source = format!(
            "fn read(a: {ty}, i: i64, end: i64) -> i64 {{ return {access} or 0 }}\nfn main() -> u8 {{ fix a = {init}\nfix value = read(a, 8, 9)\nreturn ifexpr value == 0 {{ 0 }} else {{ 1 }} }}"
        );
        let (mut program, _) =
            crate::analyze(source.as_bytes(), &crate::CompilationOptions::default()).unwrap();
        for broken in [false, true] {
            if broken {
                let mut changed = false;
                for function in &mut program.functions {
                    for statement in &mut function.statements {
                        if let CheckedStatement::Return { value, .. } = statement
                            && matches!(value.value, Value::ArrayRead { .. })
                        {
                            omit_bounds(value);
                            changed = true;
                        }
                    }
                }
                assert!(changed);
            }
            let path = directory.join(format!("{index}-{broken}.c"));
            let exe = directory.join(format!("{index}-{broken}"));
            fs::write(
                &path,
                generate(
                    &program,
                    "array-negative.suz",
                    crate::SymbolPoolLimit::default(),
                    crate::ArenaOptions::default(),
                    crate::LogLevel::default(),
                    false,
                    None,
                ),
            )
            .unwrap();
            let build = runtime_link::compiler()
                .args([
                    "-std=c23",
                    "-O1",
                    "-fsanitize=address",
                    "-fno-omit-frame-pointer",
                ])
                .arg(&path)
                .arg("-o")
                .arg(&exe)
                .output()
                .unwrap();
            assert!(
                build.status.success(),
                "{}",
                String::from_utf8_lossy(&build.stderr)
            );
            let run = Command::new(&exe)
                .env(
                    "ASAN_OPTIONS",
                    "detect_leaks=0:abort_on_error=0:exitcode=86",
                )
                .output()
                .unwrap();
            assert_eq!(
                run.status.code(),
                Some(if broken { 86 } else { 0 }),
                "{index}/{broken}: {run:?}"
            );
            assert_eq!(
                String::from_utf8_lossy(&run.stderr).contains("AddressSanitizer"),
                broken
            );
        }
    }
}
