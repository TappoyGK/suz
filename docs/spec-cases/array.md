# 配列

[目次と書式](../spec-cases.md)

## 配列の受理・拒否例（概要）

次のdiffは、同じ長さの入出力配列の隣接要素の差を計算する。各添字は [array.bounds-proof] で保証され、orは不要である。nが0または1なら書き込まず、nが2以上ならoutの最後の要素は更新しない。関数入口の長さの一致条件は空配列にも適用する。

```suz
fn diff(a: ptr [f32; n], out: ptr [f32; n]) -> void {
    for i in 1..<n {
        out#[i - 1] = a[i] - a[i - 1]
    }
}
fn main() -> u8 {
    fix ptr a = new [1.0 as f32, 4.0 as f32, 9.0 as f32]
    fix ptr out = new [0.0 as f32; 3]
    diff(a, out)
    return ifexpr out[0] == 3.0 && out[1] == 5.0 { 0 } else { 1 }
}
```

通常終了し、終了コードは0。次の各行は独立した例である。

| 前提・ソース | 期待結果・主根拠 |
|---|---|
| `fix ptr a: ptr [i32] = new []` | 受理。長さ0の非null配列。[array.initializer-type] |
| `fix ptr a = new []` | 拒否。要素型を決定できない。[array.initializer-type] |
| `new [[1, 2], [3]]` | 拒否。行の長さが不一致。[array.enumeration] |
| `new [0; -1]` | 拒否。定数の長さが負。[array.layout] |
| `a: ptr? [i32]`に対する`len a` | 拒否。非null確認が必要。[array.value] |
| 仮引数`a: ptr [i32; n]`に対する`a[0] or 7` | 受理。nは0かもしれないため失敗処理が必要。[array.access-handler] |
| 同じ仮引数に対する`a[0]` | 拒否。範囲保証がない。[array.access-handler] |
| `fix ptr a = new [1, 2]`の後の`a[0] or 7` | 拒否。既知の長さに対する成功保証あり。[array.bounds-proof] |
| `a: ptr [i32; n]`の`for i in 0..<n`内で`a[i] or 0` | 拒否。仕様上成功する添字へのor。[array.bounds-proof] |
| 同じループで、別寸法mを持つ`b: ptr [i32; m]`の`b[i]` | 拒否。寸法の一致条件がない。[array.dimension-origin] |
| 既知の長さ2のaに対する`a[2] or 7` | 受理。結果は7。範囲外の読取りは失敗処理へ進む。[array.access-handler] |
| 長さ0のaに対する`a#[0] = 1` | 実行時パニック。書き込み先の範囲外。[rt.array-bounds] |
| 長さ2と3の配列を上記diffへ渡す | 本体実行前にパニック。[rt.array-dimensions] |

次は切り出し後も2次元目のwを使える例である。切り出しが成功した場合、内側の添字にorは不要である。高さについてはsub自身の長さを使う。

```suz
fn clear_rows(grid: ptr [u8; h, w]) -> void {
    fix ptr sub = grid[1:3] or return
    for y in 0..<len sub {
        for x in 0..<w {
            sub[y]#[x] = 0
        }
    }
}
```

次は配列要素への短命な参照の保存であり、[array.lifetime]・[arena.store]で拒否する。代入を`slots#[0] = null`にすると受理する。

```suz
struct Node { value: i32 }
fn main() -> u8 {
    fix ptr slots: ptr [ptr? Node] = new [null; 1]
    arena {
        fix ptr inner = new Node { value: 1 }
        slots#[0] = inner
    }
    return 0
}
```

### array-001 空配列はnullではない

主根拠：`[array.value]`。

**ソース：**

```suz
fn main() -> u8 {
    fix ptr a: ptr? [i32] = new []
    let p = a {
        if len p == 0 { return 0 }
    }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### array-002 列挙した行の長さが一致しない

主根拠：`[array.enumeration]`。

**ソース：**

```suz
fn main() -> u8 {
    fix ptr a = new [[1, 2], [3]]
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[array.enumeration]` | 2 | 21 |

### array-003 配列の別名から要素を更新する

主根拠：`[array.write]`。

添字が実行時に渡される読み取りには、失敗時の値を指定する。

**ソース：**

```suz
fn read(a: ptr [i64; _, _], y: i64, x: i64) -> i64 {
    return a[y][x] or -1
}
fn main() -> u8 {
    fix ptr a = new [[1, 2], [3, 4]]
    fix ptr alias = a
    alias[1]#[0] = 9
    if read(a, 1, 0) != 9 { return 1 }
    if read(a, 2, 0) != -1 { return 2 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### array-004 範囲外の書き込みは右辺より先に失敗する

主根拠：`[rt.array-bounds]`。

**ソース：**

```suz
fn rhs() -> i64 { panic `right operand evaluated` }
fn main() -> u8 {
    fix ptr a = new [0; 2]
    a#[2] = rhs()
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | パニック | 101 | 0 |

**パニック診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[rt.array-bounds]` | 4 | 7 |


### array-005 部分配列と切り出しは元の実体を更新する

主根拠：`[array.slice]`。

補助根拠：`[array.index]`。

**ソース：**

```suz
fn update(a: ptr [i64; _, _], y: i64, begin: i64, end: i64) -> void {
    fix ptr row = a[y] or return
    fix ptr view = row[begin:end] or return
    view#[0] = 9
}
fn read(a: ptr [i64; _, _], y: i64, x: i64) -> i64 {
    return a[y][x] or -1
}
fn main() -> u8 {
    fix ptr a = new [[1, 2], [3, 4]]
    update(a, 1, 0, 1)
    if read(a, 1, 0) != 9 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### array-006 反復の途中の更新を後続の反復が読む

主根拠：`[array.iteration]`。

**ソース：**

```suz
fn main() -> u8 {
    fix ptr a = new [1, 2, 3]
    var sum: i64 = 0
    for value in a {
        sum += value
        a#[2] = 9
    }
    if sum != 12 { return 1 }
    var order: i64 = 0
    rev value in a { order = order * 10 + value }
    if order != 921 { return 2 }
    fix ptr empty: ptr [i64] = new []
    for value in empty { return 3 }
    rev value in empty { return 4 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |


### array-007 寸法名の一致条件と隣接要素の範囲保証

主根拠：`[array.bounds-proof]`。

**ソース：**

```suz
fn diff(a: ptr [i64; n], out: ptr [i64; n]) -> void {
    for i in 1..<n { out#[i - 1] = a[i] - a[i - 1] }
}
fn main() -> u8 {
    fix ptr a = new [1, 4, 9]
    fix ptr out = new [0; 3]
    diff(a, out)
    if out[0] != 3 || out[1] != 5 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### array-008 異なる長さの実引数は本体の前に拒否する

主根拠：`[rt.array-dimensions]`。

**ソース：**

```suz
fn same(a: ptr [i64; n], b: ptr [i64; n]) -> void {
    panic `body entered`
}
fn main() -> u8 {
    same(new [0; 2], new [0; 3])
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | パニック | 101 | 0 |

**パニック診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[rt.array-dimensions]` | 1 | 39 |

### array-009 別配列には長さの関係を移さない

主根拠：`[array.dimension-origin]`。

**ソース：**

```suz
fn main() -> u8 {
    fix ptr a = new [1; 3]
    fix ptr b = new [1; 3]
    for i in 0..<len a { fix v = b[i] }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[array.access-handler]` | 4 | 35 |

### array-010 静的な表を関数から読み、定数の要素と長さを使う

主根拠：`[array.constant]`。

補助根拠：`[array.dimension-bind]`・`[const.expression]`・`[array.bounds-proof]`・`[fn.value-call]`。

**ソース：**

```suz
fix PALETTE: [i32] = [17, 18, 19]
fix COUNT = len PALETTE
fix FIRST = PALETTE[0]
fn color(n: u32) -> i32 { return PALETTE[n % len PALETTE] or 16 }
fn first[T](a: [T]) -> T { return a[0] or panic `empty array` }
fnshape Reader([i32], u32) -> i32
fn read(a: [i32; n], i: u32) -> i32 { return a[i] or -1 }
fn main() -> u8 {
    fix a: [i32; n] = PALETTE
    var sum: i32 = 0
    for i in 0..<n { sum += a[i] }
    if COUNT != 3 || FIRST != 17 || sum != 54 { return 1 }
    var total: i32 = 0
    for i in 0..<len PALETTE { total += PALETTE[i] }
    if total != 54 { return 5 }
    fix fallback = PALETTE[9] or 7
    if fallback != 7 { return 6 }
    fix callback: static fn Reader = fn read
    if first(a) != 17 || callback(a, 1) != 18 { return 4 }
    if color(4) != 18 || read(a, 5) != -1 { return 2 }
    var selected = [9 as i32]
    selected = PALETTE
    if read(selected, 2) != 19 { return 3 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### array-011 多次元の定数配列を切り出し、空配列と反復初期化を使う

主根拠：`[array.readonly]`。

補助根拠：`[array.slice]`・`[array.iteration]`・`[array.initializer-type]`。

**ソース：**

```suz
fix GRID: [i32; _, _] = [[1, 2], [3, 4], [5, 6]]
fix LAST: [i32] = GRID[2]
fix PART: [i32; _, _] = GRID[1:3]
fn main() -> u8 {
    fix empty: [i32] = []
    fix repeated = [7; 2, 3]
    if len empty != 0 || len repeated != 2 { return 1 }
    if LAST[1] != 6 || PART[0][0] != 3 { return 2 }
    var total: i32 = 0
    for i in 0..<len PART {
        for j in 0..<len PART[i] { total += PART[i][j] }
    }
    if total != 18 { return 5 }
    var sum: i64 = 0
    rev row in repeated { for x in row { sum += x } }
    if sum != 42 { return 3 }
    fix none = [9; 0, 3]
    for row in none { return 4 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### array-012 静的な参照を含む構造体と配列を作業領域の外でも使う

主根拠：`[array.constant]`。

補助根拠：`[array.lifetime]`・`[const.expression]`・`[array.iteration]`。

**ソース：**

```suz
struct Label { caption: text, entries: [i32] }
struct Node { children: [Node] }
fix NUMBERS: [i32] = [2, 4]
fix LABELS: [Label] = [Label { caption: `開始`, entries: NUMBERS }, Label { caption: `終了`, entries: [6 as i32] }]
fix SYMBOLS: [symbol] = [`alpha`, `beta`]
fix FLOATS: [f64] = [inf; 2]
fix MIXED: [f64] = [1.0, nan, inf]
fn labels() -> [Label] { return LABELS }
fn read(a: [Label], i: u32) -> Label { return a[i] or LABELS[0] }
fn main() -> u8 {
    var caption: text = ``
    arena { caption = read(labels(), 1).caption }
    if caption != `終了` { return 1 }
    fix ptr root = new Node { children: [] }
    if len root.children != 0 { return 5 }
    fix values = labels()
    var sum: i32 = 0
    for row in values { for n in row.entries { sum += n } }
    if sum != 12 { return 2 }
    fix symbols = SYMBOLS
    var count: i32 = 0
    for item in symbols { if item == `beta` { count += 1 } }
    if count != 1 { return 3 }
    fix floats = FLOATS
    for item in floats { if item != inf { return 4 } }
    var finite: i32 = 0
    for item in MIXED { if item == 1.0 { finite += 1 } }
    if finite != 1 { return 6 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### array-013 読み取り専用の要素を書き換えない

主根拠：`[array.readonly]`。

補助根拠：`[array.write]`。

**ソース：**

```suz
fn main() -> u8 {
    fix a = [1, 2]
    a#[0] = 9
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[array.readonly]` | 3 | 7 |

### array-014 構造体の要素のフィールドを書き換えない

主根拠：`[array.readonly]`。

補助根拠：`[array.write]`・`[struct.field]`。

**ソース：**

```suz
struct Item { value: i32 }
fn main() -> u8 {
    fix a = [Item { value: 1 }]
    a[0]#value = 2
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[array.readonly]` | 4 | 6 |

### array-015 部分配列も読み取り専用になる

主根拠：`[array.readonly]`。

補助根拠：`[array.index]`・`[array.write]`。

**ソース：**

```suz
fn main() -> u8 {
    fix a = [[1, 2], [3, 4]]
    fix row = a[1]
    row#[0] = 9
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[array.readonly]` | 4 | 9 |

### array-016 定数配列の要素にローカル変数を使わない

主根拠：`[array.constant]`。

補助根拠：`[const.expression]`。

**ソース：**

```suz
fn main() -> u8 {
    var value = 3
    fix a = [value]
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[array.constant]` | 3 | 13 |

### array-017 定数配列の長さに実行時の値を使わない

主根拠：`[array.constant]`。

補助根拠：`[const.expression]`・`[array.layout]`。

**ソース：**

```suz
fn make(n: u32) -> [i32] { return [0; n] }
fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[array.constant]` | 1 | 35 |

### array-018 静的に範囲外と分かる定数添字を拒否する

主根拠：`[array.constant-index]`。

補助根拠：`[array.access-handler]`・`[const.evaluation]`。

**ソース：**

```suz
fix TABLE = [1, 2]
fn main() -> u8 {
    fix a = TABLE[2]
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[array.constant-index]` | 3 | 18 |

### array-019 定数配列に関数値を含めない

主根拠：`[array.constant]`。

補助根拠：`[fn.value-const]`。

**ソース：**

```suz
fn value() -> i32 { return 1 }
fn main() -> u8 {
    fix a = [fn value]
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[array.constant]` | 3 | 13 |

### array-020 読み取り専用の配列も比較しない

主根拠：`[array.value]`。

補助根拠：`[array.readonly]`・`[expr.equality]`。

**ソース：**

```suz
fn main() -> u8 {
    fix a = [1, 2]
    if a == a { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[array.value]` | 3 | 10 |

### array-021 配列参照をログのテンプレートに埋め込まない

主根拠：`[symbol.interpolation]`。

補助根拠：`[array.readonly]`。

**ソース：**

```suz
fn main() -> u8 {
    fix a: [i32] = [1]
    log info `v=${a}`
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[symbol.interpolation]` | 3 | 19 |

### array-022 トップレベルの定数配列もtextbufに埋め込まない

主根拠：`[symbol.interpolation]`。

補助根拠：`[array.readonly]`。

**ソース：**

```suz
fix A: [i32] = [1]
fn main() -> u8 {
    fix message = textbuf `v=${A}`
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[symbol.interpolation]` | 3 | 32 |

### array-023 読み取り専用の参照自体はnewの初期値にしない

主根拠：`[arena.new-value]`。

補助根拠：`[array.type]`。

**ソース：**

```suz
fix A: [i32] = [1]
fn main() -> u8 {
    fix ptr q: ptr [i32] = new A
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[arena.new-value]` | 3 | 28 |

### array-024 配列参照を含む構造体も比較しない

主根拠：`[array.value]`。

補助根拠：`[expr.equality]`。

**ソース：**

```suz
struct Holder { values: [i32] }
fn main() -> u8 {
    fix a = Holder { values: [1] }
    fix b = Holder { values: [1] }
    if a == b { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[array.value]` | 5 | 10 |

### array-025 別の定数配列に長さの関係を移さない

主根拠：`[array.dimension-origin]`。

補助根拠：`[array.bounds-proof]`。

**ソース：**

```suz
fix A: [i32] = [1, 2]
fix B: [i32] = [1, 2]
fn main() -> u8 {
    for i in 0..<len A { fix v = B[i] }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[array.access-handler]` | 4 | 35 |
