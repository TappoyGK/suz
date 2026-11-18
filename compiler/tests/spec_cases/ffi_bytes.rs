use crate::test_util::TemporaryDirectory;
use std::{fs, process::Command};

#[test]
fn ffi_bytes_preserve_storage_views_lifetime_and_evaluation() {
    let directory = TemporaryDirectory::new();
    // テスト用Cだけが内部ヘッダーを参照し、渡されたポインタがインターン済み実体そのものか調べる。
    fs::write(
        directory.0.join("bytes.h"),
        r#"
#include <stddef.h>
int identity(const void *, size_t);
int view(const void *, size_t, const void *, size_t);
void save(const void *, size_t);
int retained(void);
"#,
    )
    .unwrap();
    fs::write(
        directory.0.join("bytes.c"),
        r#"
#include "bytes.h"
#include "suz_text.h"
#include <string.h>
static const void *saved;
static size_t saved_length;
int identity(const void *p, size_t n) {
    suz_text original = suz_text_intern(p, n, false, (suz_text_location){"test",1,1});
    return p == suz_text_bytes(original);
}
int view(const void *base, size_t whole, const void *part, size_t n) {
    const unsigned char full[] = {65,0xe3,0x81,0x82,0,66};
    const unsigned char expected[] = {0xe3,0x81,0x82,0};
    return whole == 6 && memcmp(base, full, 6) == 0 && part == (const char *)base + 1 && n == 4 && memcmp(part, expected, 4) == 0;
}
void save(const void *p, size_t n) { saved = p; saved_length = n; }
int retained(void) { return saved_length == 3 && memcmp(saved, "x42", 3) == 0; }
"#,
    )
    .unwrap();
    let source = r#"
extern "bytes.h" {
    fn identity(data: *const void, size: _size_t) -> _int
    fn view(base: *const void, whole: _size_t, part: *const void, size: _size_t) -> _int
    fn save(data: *const void, size: _size_t) -> void
    fn retained() -> _int
}
struct Data { pointer: *const void, size: _size_t }
var CALLS: i32 = 0
fn message() -> symbol { CALLS += 1
return `Aあ\0B` }
fn make_data(value: i32) -> Data {
    fix message_text = `x${value}`
    unsafe { return Data { pointer: c_bytes message_text, size: c_byte_len message_text } }
}
fn forward(pointer: *const void) -> *const void { return pointer }
fnshape Get() -> *const void
fn get(f: fn Get) -> *const void { return f() }
fn main() -> u8 {
    fix original = message()
    fix part = original[1:3] or return 1
    if c_byte_len original != 6 || c_byte_len part != 4 || len original != 4 { return 2 }
    unsafe {
        if identity(c_bytes original, c_byte_len original) != 1 { return 3 }
        if view(c_bytes original, c_byte_len original, c_bytes part, c_byte_len part) != 1 { return 4 }
        if identity(c_bytes message(), c_byte_len message()) != 1 || CALLS != 3 { return 5 }
        fix stored = make_data(42)
        fix pointer = forward(stored.pointer)
        var current = c_bytes original
        current = get(fn [pointer]() -> *const void { return pointer })
        save(current, stored.size)
    }
    for i in 0..<100 { fix churn = `new${i}` }
    unsafe { if retained() != 1 { return 6 } }
    fix empty = original[4:4] or return 7
    if c_byte_len empty != 0 || c_byte_len `` != 0 { return 8 }
    unsafe { if identity(c_bytes ``, c_byte_len ``) != 1 { return 9 } }
    return 0
}
"#;
    let program = directory.0.join("program.c");
    fs::write(&program, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let executable = directory.0.join("program");
    let built = crate::support::runtime_link::compiler()
        .args(["-std=c23", "-Wall", "-Wextra", "-Werror", "-I"])
        .arg(&directory.0)
        .arg(&program)
        .arg(directory.0.join("bytes.c"))
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(built.status.success(), "{built:?}");
    let result = Command::new(executable).output().unwrap();
    assert_eq!(result.status.code(), Some(0), "{result:?}");
}

#[test]
fn ffi_pointer_operations_and_unsafe_boundaries_are_rejected() {
    for (body, rule) in [
        ("fix p = c_bytes `x`", "ffi.unsafe"),
        ("unsafe { }\nfix p = c_bytes `x`", "ffi.unsafe"),
        ("fix n = c_byte_len 42", "ffi.bytes-source"),
        ("unsafe { fix p = c_bytes 42 }", "ffi.bytes-source"),
        (
            "unsafe { fix p = c_bytes `x`\nfix q = p == p }",
            "ffi.pointer-operations",
        ),
        (
            "unsafe { fix p = c_bytes `x`\nfix q = p < p }",
            "ffi.pointer-operations",
        ),
        (
            "unsafe { fix p = c_bytes `x`\nfix q = p + 1 }",
            "expr.operator-types",
        ),
        (
            "unsafe { fix p = c_bytes `x`\nfix q = p as u64 or 0 }",
            "ffi.pointer-operations",
        ),
        (
            "unsafe { fix p = 0 as *const void or c_bytes `x` }",
            "ffi.pointer-operations",
        ),
        ("fix p: *const void = 0", "type.expected"),
        (
            "unsafe { fix p = c_bytes `x`\nlog info `${p}` }",
            "symbol.interpolation",
        ),
        (
            "unsafe { fix p = c_bytes `x`\nfix q = `${p}` }",
            "symbol.interpolation",
        ),
        (
            "unsafe { fix p = c_bytes `x`\nfix n = c_byte_len p }",
            "ffi.bytes-source",
        ),
        (
            "unsafe { fix a = Data { pointer: c_bytes `x` }\nfix b = a == a }",
            "ffi.pointer-operations",
        ),
        (
            "fix p = `Data { pointer: 0 }` as Data or return 1",
            "symbol.cast",
        ),
        ("unsafe { fix p: *void = c_bytes `x` }", "ffi.pointer-type"),
        (
            "unsafe { fix p: *const u8 = c_bytes `x` }",
            "ffi.pointer-type",
        ),
        (
            "unsafe { fix p = call(fn []() -> *const void { return c_bytes `x` }) }",
            "ffi.unsafe",
        ),
        (
            "unsafe { fix p: *const _int = c_bytes `x` }",
            "ffi.pointer-type",
        ),
        (
            "fix a = `abc`\nfix n = c_byte_len a + 1",
            "expr.operator-types",
        ),
        (
            "fix a = `abc`\nfix n = c_byte_len (a + 1)",
            "symbol.operations",
        ),
    ] {
        let source = format!(
            "struct Data {{ pointer: *const void }}\nfnshape Get() -> *const void\nfn call(f: fn Get) -> *const void {{ return f() }}\nfn main() -> u8 {{\n{body}\nreturn 0\n}}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), rule, "{source}: {error:?}");
    }
    let source =
        b"extern \"header.h\" { fn pointer() -> *const void }\nfn main() -> u8 { return 0 }";
    let error = suz::compile(source).unwrap_err();
    assert_eq!(error.rule(), "ffi.type");
    for operand in ["'a'", "Item { n: 1 }", "native"] {
        for operation in ["c_bytes", "c_byte_len"] {
            let source = format!(
                "struct Item {{ n: i32 }}\nfn main() -> u8 {{ fix native: _int = 1\nunsafe {{ fix n = {operation} {operand} }}\nreturn 0 }}"
            );
            let error = suz::compile(source.as_bytes()).unwrap_err();
            assert_eq!(error.rule(), "ffi.bytes-source", "{source}: {error:?}");
        }
    }
}

#[test]
fn ffi_byte_length_overflow_panics_before_truncation() {
    let directory = TemporaryDirectory::new();
    let source =
        suz::compile(b"fn main() -> u8 { fix n = c_byte_len `abcd`\nreturn n as u8 or 0 }")
            .unwrap();
    // このホストではsize_tが64ビットなので、狭い対象の上限をCマクロで再現して実際の検査を通す。
    let source = source.replace(
        "#if SIZE_MAX < UINT64_MAX",
        "#undef SIZE_MAX\n#define SIZE_MAX 3\n#if SIZE_MAX < UINT64_MAX",
    );
    let path = directory.0.join("program.c");
    fs::write(&path, source).unwrap();
    let result = crate::test_util::build_and_run_c(&path, &directory.0.join("program"));
    assert_eq!(result.status.code(), Some(101), "{result:?}");
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("rt.ffi-byte-len"),
        "{result:?}"
    );
}

#[test]
fn ffi_constant_initializers_are_source_errors() {
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("constant.suz");
    // CLIの終了コードまで検査し、内部エラーに戻らないことを確認する。
    for declaration in [
        "fix N: _size_t = c_byte_len `ab`",
        "fix T: text = `ab`
fix N: _size_t = c_byte_len T",
        "fix N: _int = 5",
        "fix N = (5 as _int or 0) as i32 or 0",
        "fix N = (5 as _int or 0) == 5",
        "struct Item { n: _int }
fix N = Item { n: 5 }",
        "var N: _int = 5",
    ] {
        let source = format!(
            "{declaration}
fn main() -> u8 {{ return 0 }}"
        );
        fs::write(&path, source).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_suz"))
            .arg("check")
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{declaration}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("[const.expression]"),
            "{declaration}: {output:?}"
        );
    }
}

#[test]
fn ffi_const_qualifier_preserves_identifiers() {
    for source in [
        "fn main() -> u8 { fix const: u8 = 0
return const }",
        "struct Item { const: u8 }
fn main() -> u8 { fix item = Item { const: 0 }
return item.const }",
        "fn const() -> u8 { return 0 }
fn main() -> u8 { return const() }",
    ] {
        suz::compile(source.as_bytes()).unwrap();
    }
}

#[test]
fn ffi_byte_length_needs_no_call_when_size_t_can_hold_length() {
    let directory = TemporaryDirectory::new();
    let source = suz::compile(
        b"fn main() -> u8 { fix n = c_byte_len `abc`
return n as u8 or 0 }",
    )
    .unwrap();
    // 最適化器に頼らず、Cの前処理だけで呼び出しが消えることを確認する。
    // size_tが狭いホストでも、ここでは検査不要な分岐を選ぶ。
    let source = source.replace(
        "#if SIZE_MAX < UINT64_MAX",
        "#undef SIZE_MAX
#define SIZE_MAX UINT64_MAX
#if SIZE_MAX < UINT64_MAX",
    );
    let path = directory.0.join("program.c");
    fs::write(&path, source).unwrap();
    let output = crate::support::runtime_link::compiler()
        .args(["-std=c23", "-O0", "-E", "-P"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("suz_ffi_byte_len"));
}
