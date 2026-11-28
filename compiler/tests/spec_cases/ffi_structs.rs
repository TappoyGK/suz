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
    fix parsed = "Sample { x: 5, y: 6.0 }" as Sample or return 2
    if parsed.x != 5 || parsed.y != 6.0 { return 3 }
    arena {
        fix p = new copied
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
        ("struct sample { int32_t x.[1]; double y; };", false),
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
        "mut i32",
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

#[test]
fn soa_extern_struct_copy_and_explicit_writeback_and_c_integer_restrictions() {
    let directory = TemporaryDirectory::new();
    fs::write(
        directory.0.join("fixture.h"),
        "#include <stdint.h>\nstruct c_item { float x; float y; long count; };\nstatic inline void bump_item(struct c_item *p) { p->x += 5.0f; p->count += 1; }\n",
    )
    .unwrap();
    fs::write(
        directory.0.join("main.suz"),
        r#"
extern "fixture.h" {
    struct c_item as Citem { x: f32, y: f32, count: _long }
    fn bump_item(p: *Citem) -> void
}
struct Wrap { item: Citem, tag: i32 }
fn main() -> u8 {
    fix items = new soa #[
        Citem { x: 1.0, y: 2.0, count: 10 },
        Citem { x: 3.0, y: 4.0, count: 20 },
    ]
    fix p = new items.[0]
    trusted {
        bump_item(p)
    }
    // C 側の書き換えは独立したコピーに対するものであり、SoA 配列自体は変わらない。
    if items.x.[0] != 1.0 || items.count.[0] != 10 { return 1 }
    // 明示的に書き戻すと SoA の各列へ反映される。
    items#[0] = .p
    if items.x.[0] != 6.0 || items.count.[0] != 11 { return 2 }

    // 入れ子に extern 構造体を含む通常構造体の SoA でも同様に動作する。
    fix wraps = new soa #[Wrap { item: items.[1], tag: 7 }]
    if wraps.item.y.[0] != 4.0 || wraps.item.count.[0] != 20 { return 3 }
    return 0
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suz"))
        .current_dir(&directory.0)
        .args(["run", "main.suz", "--include-dir", "."])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");

    // C 固有整数型の列には [ffi.c-integer] の算術制限が維持される。
    let bad_arith = b"extern \"fixture.h\" { struct c_item as Citem { count: _long } }\nfn main() -> u8 {\n    fix a = new soa #[Citem { count: 1 }]\n    fix x = a.count.[0] + 1\n    return 0\n}";
    let err = suz::compile(bad_arith).unwrap_err();
    assert_eq!(err.rule(), "expr.operator-types", "{err}");

    // 生成 C は SoA 参照に列ごとの連続ポインタ (.cols) と 1 回の領域確保を使い、
    // 葉列への一括書き込みは restrict 付きの連続ループ、16 バイト超の SoA 仮引数は const *restrict ポインタ渡し、
    // 単一葉フィールド書き込みは参照全体をコピーしない。
    let soa_codegen_src = br#"
struct Vec2 { x: f32, y: f32 }
struct Particle { pos: Vec2, vel: Vec2, life: f32 }
fn step(p: soa #[Particle; n], dt: f32) -> void {
    p.pos.x#[p.life > 0.0] += p.vel.x * dt
    for i in 0..<n {
        p.[i]#life -= dt
    }
}
fn main() -> u8 {
    fix p = new soa #[Particle { pos: Vec2 { x: 1.0, y: 2.0 }, vel: Vec2 { x: 3.0, y: 4.0 }, life: 1.0 }; 4]
    step(p, 0.5)
    return 0
}
"#;
    let c_code = suz::compile(soa_codegen_src).unwrap();
    assert!(
        c_code.contains("void *cols[5];"),
        "expected 5 leaf columns in SoA ref:\n{c_code}"
    );
    assert!(
        c_code.contains("float *restrict suz_bulk_dst_"),
        "expected restrict bulk loop on leaf column:\n{c_code}"
    );
    assert!(
        c_code.contains("const suz_array_0 *restrict suz_ref_0"),
        "expected large SoA parameter to be passed by const restrict pointer:\n{c_code}"
    );
    assert!(
        !c_code.contains("suz_soa_src_"),
        "expected single-leaf field store to avoid copying whole SoA ref:\n{c_code}"
    );
}
