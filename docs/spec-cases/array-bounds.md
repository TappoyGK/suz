# 配列（範囲保証・ハンドラ・寿命）

[目次と書式](../spec-cases.md)

### array-004 範囲外の書き込みは右辺より先に失敗する
- 主根拠：`[rt.array-bounds]`
- 結果：パニック `[rt.array-bounds]` 4:7

```suz
fn rhs() -> i64 { panic "right operand evaluated" }
fn main() -> u8 {
    fix a = new #[0; 2]
    a#[2] = rhs()
    return 0
}
```

### array-111 対象が項でない2変数反復では範囲の保証を覚えない
- 主根拠：`[array.bounds-proof]`
- 補助根拠：`[array.iteration]`、`[array.access-handler]`
- 結果：コンパイルエラー `[array.access-handler]` 4:22

```suz
fn f(a: .[i32]) -> i32 {
    if 1 <= len a {
        for i, x in a.[1:] {
            fix y = a.[i]
            return y
        }
    }
    return 0
}
fn main() -> u8 { return 0 }
```

### array-007 寸法名の一致条件と隣接要素の範囲保証
- 主根拠：`[array.bounds-proof]`
- 結果：正常終了

```suz
fn diff(a: #[i64; n], out: #[i64; n]) -> void {
    for i in 1..<n { out#[i - 1] = a.[i] - a.[i - 1] }
}
fn main() -> u8 {
    fix a = new #[1, 4, 9]
    fix out = new #[0; 3]
    diff(a, out)
    if out.[0] != 3 || out.[1] != 5 { return 1 }
    return 0
}
```

### array-009 別配列には長さの関係を移さない
- 主根拠：`[array.dimension-origin]`
- 結果：コンパイルエラー `[array.access-handler]` 4:35

```suz
fn main() -> u8 {
    fix a = new #[1; 3]
    fix b = new #[1; 3]
    for i in 0..<len a { fix v = b.[i] }
    return 0
}
```

### array-025 同じ長さの定数配列同士は整数の定数式として扱う
- 主根拠：`[array.dimension-origin]`
- 補助根拠：`[array.bounds-proof]`・`[array.access-handler]`
- 結果：正常終了

```suz
fix A: .[i32] = .[1, 2]
fix B: .[i32] = .[1, 2]
fn main() -> u8 {
    var sum: i32 = 0
    for i in 0..<len A { sum += B.[i] }
    if sum != 3 { return 1 }
    return 0
}
```

### array-028 読み取り専用の参照を作業領域の外へ残さない
- 主根拠：`[array.lifetime]`
- 補助根拠：`[array.readonly-conversion]`・`[arena.store]`
- 結果：コンパイルエラー `[arena.store]` 4:13

```suz
fn view(a: #[i32]) -> .[i32] { return a }
fn main() -> u8 {
    var saved: .[i32] = .[]
    arena { saved = view(new #[1]) }
    return 0
}
```

### array-029 変換後に読んだ要素の参照も作業領域の外へ残さない
- 主根拠：`[array.lifetime]`
- 補助根拠：`[array.readonly-conversion]`・`[arena.store]`・`[array.index]`
- 結果：コンパイルエラー `[arena.store]` 7:9

```suz
struct Item { value: i32 }
fn first(a: .[mut Item]) -> mut Item { return a.[0] or panic "empty" }
fn main() -> u8 {
    var saved: mut? Item = nil
    arena {
        fix a = new #[new Item { value: 1 }]
        saved = first(a)
    }
    return 0
}
```

### array-037 new .[...] で確保した読み取り専用配列をarenaの外へ持ち出せない
- 主根拠：`[array.lifetime]`
- 補助根拠：`[arena.store]`
- 結果：コンパイルエラー `[arena.store]` 5:9

```suz
fn main() -> u8 {
    var escaped: .[i32]? = nil
    arena {
        fix x: i32 = 3
        escaped = new .[x, 4]
    }
    return 0
}
```

### array-045 定数式である配列参照は連鎖添字・構造体フィールド・反復でも長さを既知とする
- 主根拠：`[array.bounds-proof]`
- 補助根拠：`[const.expression]`・`[array.constant-index]`
- 結果：正常終了

```suz
struct Cfg { xs: .[i32] }
fix JAGGED: .[.[i32]] = .[.[1, 2], .[3]]
fix CFG = Cfg { xs: .[10, 20] }
fix XS: .[i32] = CFG.xs
fix DEEP: .[.[.[i32]]] = .[.[.[1], .[2, 3]], .[.[4]]]
fix V: i32 = JAGGED.[1].[0]

fn main() -> u8 {
    if JAGGED.[1].[0] != 3 || V != 3 { return 1 }
    fix r = JAGGED.[1]
    if r.[0] != 3 { return 2 }
    if CFG.xs.[1] != 20 || XS.[1] != 20 { return 3 }
    if DEEP.[0].[1].[1] != 3 { return 4 }
    var total: i32 = 0
    for i in 0..<len JAGGED.[0] {
        total += JAGGED.[0].[i]
    }
    if total != 3 { return 5 }
    return 0
}
```

### array-048 作業領域の配列の要素に対する添字はorを要求する
- 主根拠：`[array.access-handler]`
- 補助根拠：`[array.bounds-proof]`
- 結果：コンパイルエラー `[array.access-handler]` 4:27

```suz
fn main() -> u8 {
    arena {
        fix a: #[.[i32]] = new #[.[1, 2], .[3]]
        fix x: i32 = a.[1].[0]
    }
    return 0
}
```

### array-049 ブロック内のfixを経由した定数配列の要素に対する添字はorを要求する
- 主根拠：`[array.access-handler]`
- 補助根拠：`[array.bounds-proof]`・`[const.expression]`
- 結果：コンパイルエラー `[array.access-handler]` 4:23

```suz
fix JAGGED: .[.[i32]] = .[.[1, 2], .[3]]
fn main() -> u8 {
    fix t = JAGGED
    fix x: i32 = t.[1].[0]
    return 0
}
```

### array-052 判定済みの条件と合流による添字・切り出しの範囲保証
- 主根拠：`[array.bounds-proof]`
- 補助根拠：`[array.index]`・`[array.slice]`・`[array.access-handler]`
- 結果：正常終了

```suz
fn color(palette: .[i32; _], n: i32) -> i32 {
    if 0 <= n < len palette {
        return palette.[n]
    }
    return 16
}

fn joined(a: .[i32; n], flag: bool, i: i64) -> i32 {
    if flag {
        if not (0 <= i < n) { return 0 }
    } else {
        if not (0 <= i < n - 1) { return 0 }
    }
    return a.[i]
}

fn main() -> u8 {
    fix pal: .[i32] = .[10, 20, 30]
    if color(pal, 1) != 20 || color(pal, 5) != 16 { return 1 }
    if joined(pal, true, 2) != 30 || joined(pal, false, 1) != 20 { return 2 }
    var i: i64 = 0
    var sum: i32 = 0
    while 0 <= i < len pal {
        sum += pal.[i]
        i += 1
    }
    if sum != 60 { return 3 }
    fix s: i64 = 1
    fix e: i64 = 3
    if 0 <= s <= e <= len pal {
        if len pal.[s:e] != 2 { return 4 }
    }
    return 0
}
```

### array-053 合流点では片方の経路だけで成り立つ強い条件を残さない
- 主根拠：`[array.bounds-proof]`
- 補助根拠：`[array.access-handler]`
- 結果：コンパイルエラー `[array.access-handler]` 7:13

```suz
fn f(a: .[i32; n], flag: bool, i: i64) -> i32 {
    if flag {
        if not (0 <= i < n) { return 0 }
    } else {
        if not (0 <= i < n - 1) { return 0 }
    }
    return a.[i + 1]
}
fn main() -> u8 {
    return 0
}
```

### array-054 保証済みのアクセスへのorは警告になる
- 主根拠：`[array.redundant-handler]`
- 補助根拠：`[array.bounds-proof]`・`[array.access-handler]`
- 結果：正常終了 / 警告：`[array.redundant-handler]` 3:28

```suz
fn color(palette: .[i32; _], n: i32) -> i32 {
    if 0 <= n < len palette {
        return palette.[n] or 16
    }
    return 16
}
fn main() -> u8 {
    if color(.[10, 20], 1) != 20 { return 1 }
    return 0
}
```

### array-055 範囲外で抜ける読み出しの後・符号なし型・剰余による範囲保証
- 主根拠：`[array.bounds-proof]`
- 補助根拠：`[array.index]`・`[array.access-handler]`
- 結果：正常終了

```suz
fn read_twice(a: .[i32], i: i64) -> i32 {
    fix v = a.[i] or return -1
    return v + a.[i]
}

fn sum_len_t(a: .[i32]) -> i32 {
    var i: len_t = 0
    var total: i32 = 0
    while i < len a {
        total += a.[i]
        i += 1
    }
    return total
}

fn pick_u32(palette: .[i32], n: u32) -> i32 {
    return palette.[n % len palette]
}

fn main() -> u8 {
    fix pal: .[i32] = .[10, 20, 30]
    if read_twice(pal, 1) != 40 || read_twice(pal, 5) != -1 { return 1 }
    if sum_len_t(pal) != 60 { return 2 }
    if pick_u32(pal, 5) != 30 { return 3 }
    return 0
}
```

### array-056 符号付き整数の0以上を確認しない剰余は範囲保証しない
- 主根拠：`[array.bounds-proof]`
- 補助根拠：`[array.access-handler]`
- 結果：コンパイルエラー `[array.access-handler]` 2:19

```suz
fn pick(palette: .[i32], n: i32) -> i32 {
    return palette.[n % len palette]
}
fn main() -> u8 {
    return 0
}
```

### array-092 値域制限型の添字で既知長配列の範囲内を静的に保証する
- 主根拠：`[array.bounds-proof]`
- 補助根拠：`[range.decl]`、`[range.bounds]`
- 結果：正常終了

値域制限型 `Channel`（`0..<16`）の仮引数や構造体フィールドを長さ 16 の既知長配列の添字に使うと、`0 <= x <= 15 < 16` が常に成り立つため `or` なしで読み出せる。

```suz
range Channel: u8 = 0..<16

struct Voice {
    ch: Channel,
}

fix TABLE: .[u8] = .[10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25]

fn lookup_param(c: Channel) -> u8 {
    return TABLE.[c]
}

fn lookup_field(v: Voice) -> u8 {
    return TABLE.[v.ch]
}

fn main() -> u8 {
    fix c: Channel = 15
    fix v = Voice { ch: 0 }
    if lookup_param(c) == 25 && lookup_field(v) == 10 {
        return 0
    }
    return 1
}
```

### array-093 値域上限が配列長を超える場合は添字にorを要求する
- 主根拠：`[array.access-handler]`
- 補助根拠：`[array.bounds-proof]`、`[range.decl]`
- 結果：コンパイルエラー `[array.access-handler]` 6:17

値域制限型 `Channel`（`0..<16`）の上限 `15` が配列長 `8` 以上であるため、`or` を省略するとコンパイルエラーとなる。

```suz
range Channel: u8 = 0..<16

fix SMALL: .[u8] = .[0, 1, 2, 3, 4, 5, 6, 7]

fn lookup(c: Channel) -> u8 {
    return SMALL.[c]
}

fn main() -> u8 {
    fix c: Channel = 3
    return lookup(c)
}
```
