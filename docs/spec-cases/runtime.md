# 実行時の検査

[目次と書式](../spec-cases.md)

### runtime-001 加算のオーバーフロー

主根拠：`[rt.int-overflow]`。

補助根拠：`[rt.panic]`。

**ソース：**

```suz
fn operation(value: u8) -> void {
    fix result = value + 1
}

fn main() -> u8 {
    operation(255)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | パニック | 101 | 0 |

加算結果256がu8の範囲外。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

**パニック診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[rt.int-overflow]` | 2 | 24 |

### runtime-002 減算のオーバーフロー

主根拠：`[rt.int-overflow]`。

補助根拠：`[rt.panic]`。

**ソース：**

```suz
fn operation(value: u8) -> void {
    fix result = value - 1
}

fn main() -> u8 {
    operation(0)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | パニック | 101 | 0 |

減算結果-1がu8の範囲外。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

**パニック診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[rt.int-overflow]` | 2 | 24 |

### runtime-003 乗算のオーバーフロー

主根拠：`[rt.int-overflow]`。

補助根拠：`[rt.panic]`。

**ソース：**

```suz
fn operation(value: u8) -> void {
    fix result = value * 2
}

fn main() -> u8 {
    operation(128)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | パニック | 101 | 0 |

乗算結果256がu8の範囲外。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

**パニック診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[rt.int-overflow]` | 2 | 24 |

### runtime-004 最小値の単項マイナス

主根拠：`[rt.int-overflow]`。

補助根拠：`[rt.panic]`。

**ソース：**

```suz
fn operation(value: i8) -> void {
    fix result = -value
}

fn main() -> u8 {
    operation(-128)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | パニック | 101 | 0 |

128がi8の範囲外。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

**パニック診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[rt.int-overflow]` | 2 | 18 |

### runtime-007 最小値を-1で割る

主根拠：`[rt.int-overflow]`。

補助根拠：`[rt.panic]`。

**ソース：**

```suz
fn operation(value: i8) -> void {
    fix result = value / -1
}

fn main() -> u8 {
    operation(-128)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | パニック | 101 | 0 |

商128をi8へ格納するときに失敗する。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

**パニック診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[rt.int-overflow]` | 2 | 24 |

### runtime-008 最小値と-1の剰余

主根拠：`[rt.int-overflow]`。

補助根拠：`[expr.int-division]`。

**ソース：**

```suz
fn operation(value: i8) -> u8 {
    fix result = value % -1
    return result as u8 or 1
}

fn main() -> u8 {
    return operation(-128)
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

剰余0はi8で表せるため、そのまま格納する。

### runtime-005 整数のゼロ除算

主根拠：`[rt.div-zero]`。

補助根拠：`[rt.panic]`。

**ソース：**

```suz
fn operation(divisor: i32) -> void {
    fix result = 7 / divisor
}

fn main() -> u8 {
    operation(0)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | パニック | 101 | 0 |

除数が0。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

**パニック診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[rt.div-zero]` | 2 | 20 |

### runtime-006 整数のゼロによる剰余

主根拠：`[rt.div-zero]`。

補助根拠：`[rt.panic]`。

**ソース：**

```suz
fn operation(divisor: i32) -> void {
    fix result = 7 % divisor
}

fn main() -> u8 {
    operation(0)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | パニック | 101 | 0 |

除数が0。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

**パニック診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[rt.div-zero]` | 2 | 20 |

### runtime-009 負のシフト数

主根拠：`[rt.shift-count]`。

補助根拠：`[rt.panic]`。

**ソース：**

```suz
fn operation(count: i32) -> void {
    fix result = 1 << count
}

fn main() -> u8 {
    operation(-1)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | パニック | 101 | 0 |

右辺が負。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

**パニック診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[rt.shift-count]` | 2 | 20 |

### runtime-010 ビット幅以上のシフト数

主根拠：`[rt.shift-count]`。

補助根拠：`[rt.panic]`。

**ソース：**

```suz
fn operation(value: u8) -> void {
    fix result = value >> 8
}

fn main() -> u8 {
    operation(1)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | パニック | 101 | 0 |

u8のビット幅と同じ8。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

**パニック診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[rt.shift-count]` | 2 | 24 |

### runtime-019 飽和左シフトの負のシフト数はパニックする

主根拠：`[rt.shift-count]`。

補助根拠：`[expr.sat-shift]`。

**ソース：**

```suz
fn main() -> u8 {
    fix shift: i32 = -1
    fix v: i8 = (1 as i8) <<| shift
    return ifexpr v == 0 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | パニック | 101 | 0 |

**パニック診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[rt.shift-count]` | 3 | 27 |

### runtime-011 左シフトの結果が範囲外

主根拠：`[rt.shift-overflow]`。

補助根拠：`[rt.panic]`。

**ソース：**

```suz
fn operation(value: u8) -> void {
    fix result = value << 1
}

fn main() -> u8 {
    operation(128)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | パニック | 101 | 0 |

256がu8の範囲外。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

**パニック診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[rt.shift-overflow]` | 2 | 24 |

### runtime-012 浮動小数点のゼロ除算は無限大かNaNになる

主根拠：`[rt.float]`。

補助根拠：`[expr.nan-comparison]`。

**ソース：**

```suz
fn main() -> u8 {
    fix zero = 0.0
    fix negative_zero = -0.0
    fix positive = 1.0 / zero
    fix negative = 1.0 / negative_zero
    fix invalid = zero / zero
    return ifexpr positive == inf && negative == -inf && invalid != invalid { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

正の値を正のゼロで割るとinf、負のゼロで割ると-infになる。0.0 / 0.0はNaNになり、自分自身との!=がtrueになる。いずれもパニックしない。

### runtime-013 f32の加算でも偶数側へ丸める

主根拠：`[rt.float-rounding]`。

補助根拠：`[type.float]`。

**ソース：**

```suz
fn main() -> u8 {
    fix base: f32 = 16_777_216.0
    fix increment: f32 = 1.0
    fix result = base + increment
    return ifexpr result == base { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

2つのオペランドはともにf32で正確に表せる。加算の数学的な結果16777217は16777216に丸められる。

### runtime-014 浮動小数点から整数への変換は切り捨て後に範囲を検査

主根拠：`[rt.as-float-int]`。

補助根拠：`[expr.cast-fallback]`。

**ソース：**

```suz
fn main() -> u8 {
    fix upper = 255.75
    fix near_zero = -0.75
    fix too_large = 256.0
    fix negative = -1.0
    fix a = upper as u8 or 9
    fix b = near_zero as u8 or 9
    fix c = too_large as u8 or 9
    fix d = negative as u8 or 9
    return ifexpr a == 255 && b == 0 && c == 9 && d == 9 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

255.75は255に、-0.75は0になってu8へ変換できる。256.0と-1.0は範囲外なので代替値になる。

### runtime-015 NaN・無限大から整数への変換失敗

主根拠：`[rt.as-float-int]`。

補助根拠：`[expr.cast-fallback]`。

**ソース：**

```suz
fn main() -> u8 {
    fix first = nan as i32 or 7
    fix second = inf as i32 or 8
    return ifexpr first == 7 && second == 8 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### runtime-016 整数からf32への変換で偶数側へ丸める

主根拠：`[rt.as-int-float]`。

補助根拠：`[rt.float-rounding]`。

**ソース：**

```suz
fn main() -> u8 {
    fix first: i32 = 16_777_217
    fix second: i32 = 16_777_219
    fix down = first as f32
    fix up = second as f32
    return ifexpr down == 16_777_216.0 && up == 16_777_220.0 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

この大きさのf32では隣り合う値の間隔が2になる。16777217は16777216へ、16777219は16777220へ丸める。どちらも距離が等しい候補のうち仮数の最下位ビットが0の側を選ぶ。

### runtime-017 enumの値が変換先の整数型に収まらない

主根拠：`[rt.as-enum-int]`。

補助根拠：`[enum.variant]`、`[expr.cast-risk]`、`[expr.cast-fallback]`。

**ソース：**

```suz
enum Code: u8 {
    V000, V001, V002, V003, V004, V005, V006, V007,
    V008, V009, V010, V011, V012, V013, V014, V015,
    V016, V017, V018, V019, V020, V021, V022, V023,
    V024, V025, V026, V027, V028, V029, V030, V031,
    V032, V033, V034, V035, V036, V037, V038, V039,
    V040, V041, V042, V043, V044, V045, V046, V047,
    V048, V049, V050, V051, V052, V053, V054, V055,
    V056, V057, V058, V059, V060, V061, V062, V063,
    V064, V065, V066, V067, V068, V069, V070, V071,
    V072, V073, V074, V075, V076, V077, V078, V079,
    V080, V081, V082, V083, V084, V085, V086, V087,
    V088, V089, V090, V091, V092, V093, V094, V095,
    V096, V097, V098, V099, V100, V101, V102, V103,
    V104, V105, V106, V107, V108, V109, V110, V111,
    V112, V113, V114, V115, V116, V117, V118, V119,
    V120, V121, V122, V123, V124, V125, V126, V127,
    V128,
}

fn convert(value: Code) -> i8 {
    fix converted = value as i8 or -1
    return converted
}

fn main() -> u8 {
    fix success = convert(Code.V127)
    fix failure = convert(Code.V128)
    return ifexpr success == 127 && failure == -1 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

基底型はu8なので、V128の値128まで宣言できる。i8へ変換すると、V127の127は成功し、V128の128は範囲外となって代替値-1を使う。

### runtime-018 整数からenumへの変換は基底型へラップしない

主根拠：`[rt.as-int-enum]`。

補助根拠：`[expr.enum-conversion]`。

**ソース：**

```suz
enum Wave: u8 { Sine, Saw }

fn main() -> u8 {
    fix raw: u16 = 256
    fix wave = raw as Wave or Wave.Saw
    return ifexpr wave == Wave.Saw { 0 } else { 1 }
}
```


**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

256をu8へラップすれば0になるが、enumへの変換では元の256を検査する。対応する列挙子がないので代替値のSawになる。
