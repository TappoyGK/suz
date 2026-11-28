# C FFI

[目次と書式](../spec-cases.md)

### ffi-001 標準C関数をtrustedから呼ぶ
- 主根拠：`[ffi.decl]`
- 結果：正常終了

```suz
extern "stdlib.h" { fn abs(value: _int) -> _int }
fn main() -> u8 {
    trusted { if abs(-7) != 7 { return 1 } }
    return 0
}
```

### ffi-002 trustedの外でC関数を呼べない
- 主根拠：`[ffi.trusted]`
- 結果：コンパイルエラー `[ffi.trusted]` 3:17

```suz
extern "stdlib.h" { fn abs(value: _int) -> _int }
fn main() -> u8 {
    fix value = abs(-7)
    return 0
}
```

### ffi-003 C関数にSuzで使う別名を付ける
- 主根拠：`[ffi.name]`
- 結果：正常終了

```suz
extern "stdlib.h" { fn labs as long_abs(value: _long) -> _long }
fn main() -> u8 {
    trusted { if long_abs(-7) != 7 { return 1 } }
    return 0
}
```

### ffi-004 ランタイム用のC名を宣言しない
- 主根拠：`[ffi.name]`
- 結果：コンパイルエラー `[ffi.name]` 1:24

```suz
extern "stdlib.h" { fn suz_test() -> void }
fn main() -> u8 {

    return 0
}
```

### ffi-005 ヘッダー名に/usr/include/stdlib.hを使えない
- 主根拠：`[ffi.header]`
- 結果：コンパイルエラー `[ffi.header]` 1:8

```suz
extern "/usr/include/stdlib.h" { fn abs(value: _int) -> _int }
fn main() -> u8 {

    return 0
}
```

### ffi-006 ヘッダー名に../stdlib.hを使えない
- 主根拠：`[ffi.header]`
- 結果：コンパイルエラー `[ffi.header]` 1:8

```suz
extern "../stdlib.h" { fn abs(value: _int) -> _int }
fn main() -> u8 {

    return 0
}
```

### ffi-007 別名を変えてもC名を重複できない
- 主根拠：`[ffi.duplicate]`
- 結果：コンパイルエラー `[ffi.duplicate]` 2:24

```suz
extern "stdlib.h" { fn abs(value: _int) -> _int }
extern "stdlib.h" { fn abs as other(value: _int) -> _int }
fn main() -> u8 {

    return 0
}
```

### ffi-008 Cの引数にsymbolを渡せない
- 主根拠：`[ffi.type]`
- 結果：コンパイルエラー `[ffi.type]` 2:25

```suz
struct Value { n: i32 }
extern "example.h" { fn consume(value: symbol) -> void }
fn main() -> u8 {

    return 0
}
```

### ffi-009 Cの引数にlen_tを渡せない
- 主根拠：`[ffi.type]`
- 結果：コンパイルエラー `[ffi.type]` 2:25

```suz
struct Value { n: i32 }
extern "example.h" { fn consume(value: len_t) -> void }
fn main() -> u8 {

    return 0
}
```

### ffi-010 Cの引数にValueを渡せない
- 主根拠：`[ffi.type]`
- 結果：コンパイルエラー `[ffi.type]` 2:25

```suz
struct Value { n: i32 }
extern "example.h" { fn consume(value: Value) -> void }
fn main() -> u8 {

    return 0
}
```

### ffi-011 C構造体のフィールドを読む
- 主根拠：`[ffi.struct]`
- 結果：正常終了

```suz
extern "time.h" { struct timespec as Timespec { tv_sec: _long, tv_nsec: _long } }
fn main() -> u8 {
    fix t = Timespec { tv_sec: 2, tv_nsec: 3 }
    if t.tv_sec != 2 || t.tv_nsec != 3 { return 1 }
    return 0
}
```

### ffi-012 C整数同士を比較する
- 主根拠：`[ffi.c-integer]`
- 結果：正常終了

```suz
fn main() -> u8 {
    fix a: _int = 1
    fix b: _int = 2
    if not (a < b) || a == b { return 1 }
    return 0
}
```

### ffi-013 C整数同士の算術は使えない
- 主根拠：`[ffi.c-integer]`
- 結果：コンパイルエラー `[expr.operator-types]` 3:15

```suz
fn main() -> u8 {
    fix a: _int = 1
    fix b = a + a
    return 0
}
```

### ffi-014 C整数から固定幅整数への変換にはorが必要
- 主根拠：`[ffi.cast]`
- 結果：コンパイルエラー `[expr.cast-handler]` 3:15

```suz
fn main() -> u8 {
    fix a: _int = 1
    fix b = a as i32
    return 0
}
```

### ffi-015 C整数から浮動小数点数へ変換する
- 主根拠：`[ffi.cast]`
- 結果：正常終了

```suz
fn main() -> u8 {
    fix a: _int = 7
    fix b = a as f64
    if b != 7.0 { return 1 }
    return 0
}
```

### ffi-016 C関数を関数値にできない
- 主根拠：`[ffi.function-value]`
- 結果：コンパイルエラー `[ffi.function-value]` 3:20

```suz
extern "stdlib.h" { fn abs(value: _int) -> _int }
fn main() -> u8 {
    fix callback = fn abs
    return 0
}
```

### ffi-017 c_bytesはtrusted内で使う
- 主根拠：`[ffi.bytes]`
- 結果：コンパイルエラー `[ffi.trusted]` 2:13

```suz
fn main() -> u8 {
    fix p = c_bytes "hello"
    return 0
}
```

### ffi-018 UTF-8のバイト長をtrustedの外で得る
- 主根拠：`[ffi.byte-len]`
- 結果：正常終了

```suz
fn main() -> u8 {
    if c_byte_len "こんにちは" != 15 { return 1 }
    return 0
}
```

### ffi-019 Cバイトポインタを比較できない
- 主根拠：`[ffi.pointer-operations]`
- 結果：コンパイルエラー `[ffi.pointer-operations]` 3:23

```suz
fn main() -> u8 {
    trusted { fix p = c_bytes "hello"
        fix equal = p == p }
    return 0
}
```

### ffi-020 通常の構造体をCの型付きポインタにしない
- 主根拠：`[ffi.arena-pointer]`
- 結果：コンパイルエラー `[ffi.pointer-type]` 2:30

```suz
struct Value { n: i32 }
extern "example.h" { fn read(p: *const Value) -> void }
fn main() -> u8 {

    return 0
}
```

### ffi-021 値渡しにkeepを付けない
- 主根拠：`[ffi.keep]`
- 結果：コンパイルエラー `[ffi.keep]` 1:30

```suz
extern "example.h" { fn save(keep value: i32) -> void }
fn main() -> u8 {

    return 0
}
```

### ffi-022 書き込み可能なCポインタ仮引数にrefポインタを渡せない
- 主根拠：`[ffi.arena-pointer]`
- 補助根拠：`[type.expected]`
- 結果：コンパイルエラー `[type.expected]` 5:29

```suz
extern "example.h" { fn set_value(p: *i32) -> void }
fn main() -> u8 {
    arena {
        fix r: ref i32 = new (1 as i32)
        trusted { set_value(r) }
    }
    return 0
}
```
