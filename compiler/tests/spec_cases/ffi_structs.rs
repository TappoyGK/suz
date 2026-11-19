use crate::test_util::TemporaryDirectory;
use std::{fs, process::Command};

#[test]
fn c_struct_values_use_header_members_through_suz_operations() {
    let directory = TemporaryDirectory::new();
    fs::write(directory.0.join("fixture.h"), "#include <stdint.h>\nstruct sample { int32_t x; double y; };\nstruct Native { long count; };\n").unwrap();
    fs::write(
        directory.0.join("main.suz"),
        r#"
struct Wrapper { value: Sample }
extern "fixture.h" {
    struct sample as Sample { x: i32, y: f64 }
    struct Native { count: _long }
}
fix SAMPLE = Sample { x: 1, y: 2.0 }
fn Sample#shift() -> void { self#x += 1 }
fn copy(value: Sample) -> Sample { return value with { y: 4.0 } }
fn main() -> u8 {
    var wrap = Wrapper { value: SAMPLE }
    wrap.value#x = 3
    wrap.value#shift()
    fix copied = copy(wrap.value)
    if copied != (Sample { x: 4, y: 4.0 }) { return 1 }
    fix parsed = `Sample { x: 5, y: 6.0 }` as Sample or return 2
    if parsed.x != 5 || parsed.y != 6.0 { return 3 }
    arena {
        fix ptr p = new copied
        p#x = 8
        if p.x != 8 { return 4 }
    }
    var native = Native { count: 7 }
    native#count = 9
    if native.count != 9 { return 5 }
    return 0
}
"#,
    )
    .unwrap();
    // ヘッダーの実際の型とメンバーを使うことを、CLIからCのビルド・実行まで確認する。
    let output = Command::new(env!("CARGO_BIN_EXE_suz"))
        .current_dir(&directory.0)
        .args(["run", "main.suz", "--include-dir", "."])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn c_struct_headers_check_unused_layout_types_and_qualifiers() {
    let directory = TemporaryDirectory::new();
    let source = b"extern \"fixture.h\" { struct sample as Sample { x: i32, y: f64 } }\nfn main() -> u8 { return 0 }";
    fs::write(directory.0.join("program.c"), suz::compile(source).unwrap()).unwrap();
    // Suzだけでは判定できない実際のヘッダーとの不一致を、未使用の宣言でも拒否する。
    for (declaration, accepted) in [
        ("struct sample { int32_t x; double y; };", true),
        ("struct sample { uint32_t x; double y; };", false),
        ("struct sample { float x; double y; };", false),
        ("struct sample { double y; int32_t x; };", false),
        ("struct sample { int32_t x; double y; int32_t z; };", false),
        (
            "struct __attribute__((packed)) sample { int32_t x; double y; };",
            false,
        ),
        (
            "struct __attribute__((aligned(32))) sample { int32_t x; double y; };",
            false,
        ),
        ("struct sample { const int32_t x; double y; };", false),
        ("struct sample { volatile int32_t x; double y; };", false),
        ("struct sample { _Atomic(int32_t) x; double y; };", false),
        ("struct sample { int32_t x: 8; double y; };", false),
        ("struct sample { int32_t x[1]; double y; };", false),
        ("struct sample { int32_t *x; double y; };", false),
        ("struct sample;", false),
        ("typedef struct { int32_t x; double y; } sample;", false),
        (
            "struct other { int32_t x; double y; };\n#define sample other",
            false,
        ),
        (
            "struct sample { int32_t renamed; double y; };\n#define x renamed",
            false,
        ),
    ] {
        fs::write(
            directory.0.join("fixture.h"),
            format!("#include <stdint.h>\n{declaration}\n"),
        )
        .unwrap();
        let output = Command::new(std::env::var_os("CC").unwrap_or_else(|| "clang".into()))
            .args([
                "-std=c23",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fsyntax-only",
                "-I",
            ])
            .arg(&directory.0)
            .arg("-I")
            .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime"))
            .arg(directory.0.join("program.c"))
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            accepted,
            "{declaration}: {output:?}"
        );
    }
}

#[test]
fn c_struct_declarations_reject_unsupported_fields_and_conflicts() {
    for (declaration, rule) in [
        ("extern \"f.h\" { struct lower { x: i32 } }", "name.type"),
        (
            "extern \"f.h\" { struct sample as Sample {} }",
            "struct.nonempty",
        ),
        (
            "extern \"f.h\" { struct sample as Sample { x: i32, x: i32 } }",
            "struct.field",
        ),
        (
            "extern \"f.h\" { struct sample as Sample { x: i32 }\nstruct sample as Other { x: i32 } }",
            "ffi.duplicate",
        ),
        (
            "struct Sample { x: i32 }\nextern \"f.h\" { struct sample as Sample { x: i32 } }",
            "decl.duplicate",
        ),
        (
            "extern \"f.h\" { struct suz_private as Public { x: i32 } }",
            "ffi.name",
        ),
    ] {
        let source = format!("{declaration}\nfn main() -> u8 {{ return 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), rule, "{source}: {error}");
    }
    for ty in [
        "bool",
        "len_t",
        "char",
        "text",
        "symbol",
        "ptr i32",
        "*const void",
        "Item",
    ] {
        let source = format!(
            "struct Item {{ n: i32 }}\nextern \"f.h\" {{ struct sample as Sample {{ x: {ty} }} }}\nfn main() -> u8 {{ return 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "ffi.struct", "{ty}: {error}");
    }
}
