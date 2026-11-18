use crate::test_util::TemporaryDirectory;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn suz(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_suz"));
    command.current_dir(root);
    for name in ["CFLAGS", "CPPFLAGS", "LDFLAGS", "SUZ_CFLAGS"] {
        command.env_remove(name);
    }
    command
}
fn success(output: Output) {
    assert!(output.status.success(), "{output:?}");
}
fn cc() -> Command {
    Command::new(std::env::var_os("CC").unwrap_or_else(|| "clang".into()))
}

#[test]
fn ffi_external_objects_libraries_and_exported_paths() {
    let root = TemporaryDirectory::new();
    // シェルとして解釈したり、入力ソースの場所を相対パスの基準にすると失敗する配置。
    let vendor_name = "vendor '$(touch injected) space";
    let vendor = root.0.join(vendor_name);
    fs::create_dir(&vendor).unwrap();
    fs::create_dir(root.0.join("src")).unwrap();
    fs::write(vendor.join("bridge.h"), "int bridge(int value);\n").unwrap();
    fs::write(
        vendor.join("bridge.c"),
        "int dependency(int);\nint bridge(int x) { return dependency(x) + 1; }\n",
    )
    .unwrap();
    fs::write(
        vendor.join("dep.c"),
        "int dependency(int x) { return 2 * x; }\n",
    )
    .unwrap();
    for name in ["bridge", "dep"] {
        success(
            cc().arg("-c")
                .arg(vendor.join(format!("{name}.c")))
                .arg("-o")
                .arg(vendor.join(format!("{name}.o")))
                .output()
                .unwrap(),
        );
    }
    success(
        Command::new("ar")
            .arg("rcs")
            .arg(vendor.join("libdep.a"))
            .arg(vendor.join("dep.o"))
            .output()
            .unwrap(),
    );
    fs::write(
        root.0.join("src/main.suz"),
        r#"
extern "bridge.h" { fn bridge(value: _int) -> _int }
fn main() -> u8 { unsafe { if bridge(20) != 41 { return 1 } }
return 0 }
"#,
    )
    .unwrap();
    let object = format!("{vendor_name}/bridge.o");
    let flags = [
        "--include-dir",
        vendor_name,
        "--library-dir",
        vendor_name,
        "--link-object",
        &object,
        "--link-library",
        "dep",
    ];
    success(
        suz(&root.0)
            .args(["build", "src/main.suz", "-o", "app"])
            .args(flags)
            .output()
            .unwrap(),
    );
    success(Command::new(root.0.join("app")).output().unwrap());
    success(
        suz(&root.0)
            .args(["run", "src/main.suz"])
            .args(flags)
            .output()
            .unwrap(),
    );
    success(
        suz(&root.0)
            .args(["emit-c", "src/main.suz", "-o", "emitted"])
            .args(flags)
            .output()
            .unwrap(),
    );
    let elsewhere = TemporaryDirectory::new();
    fs::rename(root.0.join("emitted"), elsewhere.0.join("moved")).unwrap();
    let moved = elsewhere.0.join("moved");
    let mut build = Command::new(moved.join("scripts/build-c.sh"));
    build
        .current_dir(&elsewhere.0)
        .arg(moved.join("program.c"))
        .args(["-o", "app"]);
    for name in ["CFLAGS", "CPPFLAGS", "LDFLAGS", "SUZ_CFLAGS"] {
        build.env_remove(name);
    }
    success(build.output().unwrap());
    success(Command::new(elsewhere.0.join("app")).output().unwrap());
    assert!(!elsewhere.0.join("injected").exists());
    assert!(!root.0.join("injected").exists());
    assert!(!moved.join(vendor_name).exists());

    // 保存せずに、ビルドスクリプトへ直接渡す場合も実行時の作業ディレクトリを基準にする。
    success(
        suz(&root.0)
            .args(["emit-c", "src/main.suz", "-o", "plain"])
            .output()
            .unwrap(),
    );
    let mut direct = Command::new(root.0.join("plain/scripts/build-c.sh"));
    direct
        .current_dir(&root.0)
        .args(["plain/program.c", "-o", "direct"])
        .args(flags);
    for name in ["CFLAGS", "CPPFLAGS", "LDFLAGS", "SUZ_CFLAGS"] {
        direct.env_remove(name);
    }
    success(direct.output().unwrap());
    success(Command::new(root.0.join("direct")).output().unwrap());

    // 未解決シンボルと競合定義は、Suz型検査の成功とは別にリンクで失敗する。
    for extra in [
        vec![],
        vec![
            "--link-object",
            &object,
            "--link-library",
            "dep",
            "--link-object",
            &object,
        ],
    ] {
        let result = suz(&root.0)
            .args([
                "build",
                "src/main.suz",
                "-o",
                "failed",
                "--include-dir",
                vendor_name,
                "--library-dir",
                vendor_name,
            ])
            .args(extra)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(4), "{result:?}");
        assert!(!root.0.join("failed").exists());
    }
}

#[test]
fn ffi_options_validate_without_requiring_c_tools_or_files() {
    let root = TemporaryDirectory::new();
    fs::write(
        root.0.join("main.suz"),
        "extern \"absent.h\" { fn missing() -> void }\nfn main() -> u8 { return 0 }\n",
    )
    .unwrap();
    let flags = [
        "--include-dir",
        "missing/include",
        "--library-dir",
        "missing/lib",
        "--link-object",
        "missing.o",
        "--link-library",
        "missing",
    ];
    success(
        suz(&root.0)
            .env("CC", "/missing/compiler")
            .args(["check", "main.suz"])
            .args(flags)
            .output()
            .unwrap(),
    );
    success(
        suz(&root.0)
            .env("CC", "/missing/compiler")
            .args(["emit-c", "main.suz", "-o", "emitted"])
            .args(flags)
            .output()
            .unwrap(),
    );
    for option in [
        "--include-dir",
        "--library-dir",
        "--link-object",
        "--link-library",
    ] {
        for values in [vec![], vec![""], vec!["--help"]] {
            let result = suz(&root.0)
                .args(["check", "main.suz", option])
                .args(values)
                .output()
                .unwrap();
            assert_eq!(result.status.code(), Some(2), "{result:?}");
            assert!(
                String::from_utf8_lossy(&result.stderr).contains("tool.ffi-options"),
                "{result:?}"
            );
        }
    }
    for name in ["../dep", "dep name", "dep;echo", "@args"] {
        let result = suz(&root.0)
            .args(["check", "main.suz", "--link-library", name])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2), "{result:?}");
    }
}

#[cfg(unix)]
#[test]
fn ffi_link_order_is_preserved_at_the_c_driver_boundary() {
    use std::os::unix::fs::PermissionsExt;
    let root = TemporaryDirectory::new();
    fs::write(root.0.join("main.suz"), "fn main() -> u8 { return 0 }\n").unwrap();
    // macOSのリンカーはライブラリを再探索するので、成否では順序の違いを識別できない。
    // 実際にClangを実行するラッパーで、ビルド時のargvを記録する。
    let wrapper = root.0.join("cc");
    fs::write(
        &wrapper,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$TRACE\"\nexec \"$REAL_CC\" \"$@\"\n",
    )
    .unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(root.0.join("one.c"), "int one(void) { return 1; }\n").unwrap();
    success(
        cc().args(["-c"])
            .arg(root.0.join("one.c"))
            .arg("-o")
            .arg(root.0.join("one.o"))
            .output()
            .unwrap(),
    );
    let trace = root.0.join("trace");
    success(
        suz(&root.0)
            .env("TRACE", &trace)
            .env(
                "REAL_CC",
                std::env::var_os("CC").unwrap_or_else(|| "clang".into()),
            )
            .args([
                "build",
                "main.suz",
                "-o",
                "app",
                "--cc",
                "./cc",
                "--link-library",
                "m",
                "--link-object",
                "one.o",
                "--link-library",
                "m",
            ])
            .output()
            .unwrap(),
    );
    let args = fs::read_to_string(&trace).unwrap();
    let expected = format!(
        "-lm\n-Xlinker\n{}\n-lm\n",
        root.0.canonicalize().unwrap().join("one.o").display()
    );
    assert!(args.contains(&expected), "{args}");
    success(
        suz(&root.0)
            .args([
                "emit-c",
                "main.suz",
                "-o",
                "emitted",
                "--link-library",
                "m",
                "--link-object",
                "one.o",
                "--link-library",
                "m",
            ])
            .output()
            .unwrap(),
    );
    let emitted = root.0.join("emitted");
    let mut build = Command::new(emitted.join("scripts/build-c.sh"));
    build
        .current_dir(&emitted)
        .env("CC", &wrapper)
        .env("TRACE", &trace)
        .env(
            "REAL_CC",
            std::env::var_os("CC").unwrap_or_else(|| "clang".into()),
        )
        .args(["program.c", "-o", "app"]);
    for name in ["CFLAGS", "CPPFLAGS", "LDFLAGS", "SUZ_CFLAGS"] {
        build.env_remove(name);
    }
    success(build.output().unwrap());
    let args = fs::read_to_string(&trace).unwrap();
    assert!(args.contains(&expected), "{args}");
}
