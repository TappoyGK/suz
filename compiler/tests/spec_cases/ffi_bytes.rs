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
    const unsigned char expected[] = {0xe3,0x81,0x82,0};
    return whole == 6 && part == (const char *)base + 1 && n == 4 && memcmp(part, expected, 4) == 0;
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
    for body in [
        "fix p = c_bytes `x`",
        "unsafe { }\nfix p = c_bytes `x`",
        "fix n = c_byte_len 42",
        "unsafe { fix p = c_bytes 42 }",
        "unsafe { fix p = c_bytes `x`\nfix q = p == p }",
        "unsafe { fix p = c_bytes `x`\nfix q = p < p }",
        "unsafe { fix p = c_bytes `x`\nfix q = p + 1 }",
        "unsafe { fix p = c_bytes `x`\nfix q = p as u64 or 0 }",
        "unsafe { fix p = 0 as *const void or c_bytes `x` }",
        "fix p: *const void = 0",
        "unsafe { fix p = c_bytes `x`\nlog info `${p}` }",
        "unsafe { fix p = c_bytes `x`\nfix q = `${p}` }",
        "unsafe { fix p = c_bytes `x`\nfix n = c_byte_len p }",
        "unsafe { fix a = Data { pointer: c_bytes `x` }\nfix b = a == a }",
        "fix p = `Data { pointer: 0 }` as Data or return 1",
        "unsafe { fix p: *void = c_bytes `x` }",
        "unsafe { fix p: *const u8 = c_bytes `x` }",
        "unsafe { fix p = call(fn []() -> *const void { return c_bytes `x` }) }",
    ] {
        let source = format!(
            "struct Data {{ pointer: *const void }}\nfnshape Get() -> *const void\nfn call(f: fn Get) -> *const void {{ return f() }}\nfn main() -> u8 {{\n{body}\nreturn 0\n}}"
        );
        assert!(
            suz::compile(source.as_bytes()).is_err(),
            "accepted: {source}"
        );
    }
    let source =
        b"extern \"header.h\" { fn pointer() -> *const void }\nfn main() -> u8 { return 0 }";
    let error = suz::compile(source).unwrap_err();
    assert!(error.message().contains("ffi.type"), "{error:?}");
}

#[test]
fn ffi_byte_length_overflow_panics_before_truncation() {
    let directory = TemporaryDirectory::new();
    let source =
        suz::compile(b"fn main() -> u8 { fix n = c_byte_len `abcd`\nreturn n as u8 or 0 }")
            .unwrap();
    // このホストではsize_tが64ビットなので、狭い対象の上限をCマクロで再現して実際の検査を通す。
    let source = source.replace(
        "static inline size_t suz_ffi_byte_len",
        "#undef SIZE_MAX\n#define SIZE_MAX 3\nstatic inline size_t suz_ffi_byte_len",
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
