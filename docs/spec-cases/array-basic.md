# 配列（基本・定数配列・読み取り専用・反復）

[目次と書式](../spec-cases.md)

## 配列の受理・拒否例（概要）

次のdiffは、同じ長さの入出力配列の隣接要素の差を計算する。各添字は [array.bounds-proof] で保証され、orは不要である。nが0または1なら書き込まず、nが2以上ならoutの最後の要素は更新しない。関数入口の長さの一致条件は空配列にも適用する。

```suz
fn diff(a: #[f32; n], out: #[f32; n]) -> void {
    for i in 1..<n {
        out#[i - 1] = a.[i] - a.[i - 1]
    }
}
fn main() -> u8 {
    fix a = new #[1.0 as f32, 4.0 as f32, 9.0 as f32]
    fix out = new #[0.0 as f32; 3]
    diff(a, out)
    return ifexpr out.[0] == 3.0 && out.[1] == 5.0 { 0 } else { 1 }
}
```

通常終了し、終了コードは0。次の各行は独立した例である。

| 前提・ソース | 期待結果・主根拠 |
|---|---|
| `fix a: #[i32] = new #[]` | 受理。長さ0の非nil配列。[array.initializer-type] |
| `fix a = new #[]` | 拒否。要素型を決定できない。[array.initializer-type] |
| `new #[[1, 2], [3]]` | 拒否。行の長さが不一致。[array.enumeration] |
| `new #[0; -1]` | 拒否。定数の長さが負。[array.layout] |
| `a: #[i32]?`に対する`len a` | 拒否。非nil確認が必要。[array.value] |
| 仮引数`a: #[i32; n]`に対する`a.[0] or 7` | 受理。nは0かもしれないため失敗処理が必要。[array.access-handler] |
| 同じ仮引数に対する`a.[0]` | 拒否。範囲保証がない。[array.access-handler] |
| `fix a = new #[1, 2]`の後の`a.[0] or 7` | 受理（警告）。既知の長さに対する成功保証あり。[array.redundant-handler] |
| `a: #[i32; n]`の`for i in 0..<n`内で`a.[i] or 0` | 受理（警告）。仕様上成功する添字へのor。[array.redundant-handler] |
| 同じループで、別寸法mを持つ`b: #[i32; m]`の`b.[i]` | 拒否。寸法の一致条件がない。[array.dimension-origin] |
| 既知の長さ2のaに対する`a.[2] or 7` | 受理。結果は7。範囲外の読取りは失敗処理へ進む。[array.access-handler] |
| 長さ0のaに対する`a#[0] = 1` | 実行時パニック。書き込み先の範囲外。[rt.array-bounds] |
| 長さ2と3の配列を上記diffへ渡す | 本体実行前にパニック。[rt.array-dimensions] |

次は切り出し後も2次元目のwを使える例である。切り出しが成功した場合、内側の添字にorは不要である。高さについてはsub自身の長さを使う。

```suz
fn clear_rows(grid: #[u8; h, w]) -> void {
    fix sub = grid.[1:3] or return
    for y in 0..<len sub {
        for x in 0..<w {
            sub.[y]#[x] = 0
        }
    }
}
```

次は配列要素への短命な参照の保存であり、[array.lifetime]・[arena.store]で拒否する。代入を`slots#[0] = nil`にすると受理する。

```suz
struct Node { value: i32 }
fn main() -> u8 {
    fix slots: #[mut? Node] = new #[nil; 1]
    arena {
        fix inner = new Node { value: 1 }
        slots#[0] = inner
    }
    return 0
}
```

### array-001 空配列はnilではない
- 主根拠：`[array.value]`
- 結果：正常終了

```suz
fn main() -> u8 {
    fix a: #[i32]? = new #[]
    let p = a {
        if len p == 0 { return 0 }
    }
    return 1
}
```

### array-002 列挙した行の長さが一致しない
- 主根拠：`[array.enumeration]`
- 結果：コンパイルエラー `[array.enumeration]` 2:18

```suz
fn main() -> u8 {
    fix a = new #[[1, 2], [3]]
    return 0
}
```

### array-003 配列の別名から要素を更新する
- 主根拠：`[array.write]`
- 結果：正常終了

添字が実行時に渡される読み取りには、失敗時の値を指定する。

```suz
fn read(a: #[i64; _, _], y: i64, x: i64) -> i64 {
    return a.[y].[x] or -1
}
fn main() -> u8 {
    fix a = new #[[1, 2], [3, 4]]
    fix alias = a
    alias.[1]#[0] = 9
    if read(a, 1, 0) != 9 { return 1 }
    if read(a, 2, 0) != -1 { return 2 }
    return 0
}
```

### array-005 部分配列と切り出しは元の実体を更新する
- 主根拠：`[array.slice]`
- 補助根拠：`[array.index]`
- 結果：正常終了

```suz
fn update(a: #[i64; _, _], y: i64, begin: i64, end: i64) -> void {
    fix row = a.[y] or return
    fix view = row.[begin:end] or return
    view#[0] = 9
}
fn read(a: #[i64; _, _], y: i64, x: i64) -> i64 {
    return a.[y].[x] or -1
}
fn main() -> u8 {
    fix a = new #[[1, 2], [3, 4]]
    update(a, 1, 0, 1)
    if read(a, 1, 0) != 9 { return 1 }
    return 0
}
```

### array-006 反復の途中の更新を後続の反復が読む
- 主根拠：`[array.iteration]`
- 結果：正常終了

```suz
fn main() -> u8 {
    fix a = new #[1, 2, 3]
    var sum: i64 = 0
    for value in a {
        sum += value
        a#[2] = 9
    }
    if sum != 12 { return 1 }
    var order: i64 = 0
    rev value in a { order = order * 10 + value }
    if order != 921 { return 2 }
    fix empty: #[i64] = new #[]
    for value in empty { return 3 }
    rev value in empty { return 4 }
    return 0
}
```

### array-110 通常配列・多次元配列・切り出しの2変数反復
- 主根拠：`[array.iteration]`
- 補助根拠：`[array.bounds-proof]`
- 結果：正常終了

```suz
fn add_same(src: .[i32; n], out: #[i32; n]) -> void {
    for i, x in src {
        out#[i] = out.[i] + x or 0
    }
}

fn main() -> u8 {
    fix src: .[i32] = new .[1, 2, 3]
    fix out: #[i32] = new #[10, 20, 30]
    add_same(src, out)
    if out.[0] != 11 || out.[1] != 22 || out.[2] != 33 { return 1 }

    var rev_idx: i32 = 0
    rev i, x in src {
        rev_idx = rev_idx * 100 + (i as i32) * 10 + x or return 2
    }
    if rev_idx != 231201 { return 3 }

    // 切り出しの添字は切り出し先頭を0とする位置であり、元配列のオフセットは付かない
    fix base: #[i32] = new #[10, 20, 30, 40]
    var slice_acc: i32 = 0
    for i, x in base.[1:] {
        slice_acc = slice_acc + (i as i32) * 100 + x or return 4
    }
    if slice_acc != 390 { return 5 }

    fix grid: .[i32; _, _] = .[[1, 2], [3, 4]]
    var grid_sum: i32 = 0
    for y, row in grid {
        for x, v in row {
            grid_sum = grid_sum + (y as i32) * 10 + (x as i32) + v or return 6
        }
    }
    if grid_sum != 32 { return 7 }
    return 0
}
```

### array-008 異なる長さの実引数は本体の前に拒否する
- 主根拠：`[rt.array-dimensions]`
- 結果：パニック `[rt.array-dimensions]` 1:33

```suz
fn same(a: #[i64; n], b: #[i64; n]) -> void {
    panic "body entered"
}
fn main() -> u8 {
    same(new #[0; 2], new #[0; 3])
    return 0
}
```

### array-010 静的な表を関数から読み、定数の要素と長さを使う
- 主根拠：`[array.constant]`
- 補助根拠：`[array.dimension-bind]`・`[const.expression]`・`[array.bounds-proof]`・`[fn.value-call]`・`[array.constant-index]`
- 結果：正常終了

```suz
fix PALETTE: .[i32] = .[17, 18, 19]
fix COUNT = len PALETTE
fix FIRST = PALETTE.[0]
fn color(n: u32) -> i32 { return PALETTE.[n % len PALETTE] }
fn first[T](a: .[T]) -> T { return a.[0] or panic "empty array" }
fnshape Reader(.[i32], u32) -> i32
fn read(a: .[i32; n], i: u32) -> i32 { return a.[i] or -1 }
fn main() -> u8 {
    fix a: .[i32; n] = PALETTE
    var sum: i32 = 0
    for i in 0..<n { sum += a.[i] }
    if COUNT != 3 || FIRST != 17 || sum != 54 { return 1 }
    var total: i32 = 0
    for i in 0..<len PALETTE { total += PALETTE.[i] }
    if total != 54 { return 5 }
    fix fallback = PALETTE.[9] or 7
    if fallback != 7 { return 6 }
    fix callback: static fn Reader = fn read
    if first(a) != 17 || callback(a, 1) != 18 { return 4 }
    if color(4) != 18 || read(a, 5) != -1 { return 2 }
    var selected = .[9 as i32]
    selected = PALETTE
    if read(selected, 2) != 19 { return 3 }
    return 0
}
```

### array-011 多次元の定数配列を切り出し、空配列と反復初期化を使う
- 主根拠：`[array.readonly]`
- 補助根拠：`[array.slice]`・`[array.iteration]`・`[array.initializer-type]`
- 結果：正常終了

```suz
fix GRID: .[i32; _, _] = .[[1, 2], [3, 4], [5, 6]]
fix LAST: .[i32] = GRID.[2]
fix PART: .[i32; _, _] = GRID.[1:3]
fn main() -> u8 {
    fix empty: .[i32] = .[]
    fix repeated = .[7; 2, 3]
    if len empty != 0 || len repeated != 2 { return 1 }
    if LAST.[1] != 6 || PART.[0].[0] != 3 { return 2 }
    var total: i32 = 0
    for i in 0..<len PART {
        for j in 0..<len PART.[i] { total += PART.[i].[j] }
    }
    if total != 18 { return 5 }
    var sum: i64 = 0
    rev row in repeated { for x in row { sum += x } }
    if sum != 42 { return 3 }
    fix none = .[9; 0, 3]
    for row in none { return 4 }
    return 0
}
```

### array-012 静的な参照を含む構造体と配列を作業領域の外でも使う
- 主根拠：`[array.constant]`
- 補助根拠：`[array.lifetime]`・`[const.expression]`・`[array.iteration]`・`[arena.recursive-struct]`・`[arena.new-value]`・`[runtime.constant-array]`
- 結果：正常終了

```suz
struct Label { caption: text, entries: .[i32] }
struct Node { children: .[Node] }
fix NUMBERS: .[i32] = .[2, 4]
fix LABELS: .[Label] = .[Label { caption: "開始", entries: NUMBERS }, Label { caption: "終了", entries: .[6 as i32] }]
fix SYMBOLS: .[symbol] = .["alpha", "beta"]
fix FLOATS: .[f64] = .[inf; 2]
fix MIXED: .[f64] = .[1.0, nan, inf]
fn labels() -> .[Label] { return LABELS }
fn read(a: .[Label], i: u32) -> Label { return a.[i] or LABELS.[0] }
fn main() -> u8 {
    var caption: text = ""
    arena { caption = read(labels(), 1).caption }
    if caption != "終了" { return 1 }
    fix root = new Node { children: .[] }
    if len root.children != 0 { return 5 }
    fix values = labels()
    var sum: i32 = 0
    for row in values { for n in row.entries { sum += n } }
    if sum != 12 { return 2 }
    fix symbols = SYMBOLS
    var count: i32 = 0
    for item in symbols { if item == "beta" { count += 1 } }
    if count != 1 { return 3 }
    fix floats = FLOATS
    for item in floats { if item != inf { return 4 } }
    var finite: i32 = 0
    for item in MIXED { if item == 1.0 { finite += 1 } }
    if finite != 1 { return 6 }
    return 0
}
```

### array-013 読み取り専用の要素を書き換えない
- 主根拠：`[array.readonly]`
- 補助根拠：`[array.write]`
- 結果：コンパイルエラー `[array.readonly]` 3:7

```suz
fn main() -> u8 {
    fix a = .[1, 2]
    a#[0] = 9
    return 0
}
```

### array-014 構造体の要素のフィールドを書き換えない
- 主根拠：`[array.readonly]`
- 補助根拠：`[array.write]`・`[struct.field]`
- 結果：コンパイルエラー `[array.readonly]` 4:6

```suz
struct Item { value: i32 }
fn main() -> u8 {
    fix a = .[Item { value: 1 }]
    a.[0]#value = 2
    return 0
}
```

### array-015 部分配列も読み取り専用になる
- 主根拠：`[array.readonly]`
- 補助根拠：`[array.index]`・`[array.write]`
- 結果：コンパイルエラー `[array.readonly]` 4:9

```suz
fn main() -> u8 {
    fix a = .[[1, 2], [3, 4]]
    fix row = a.[1]
    row#[0] = 9
    return 0
}
```

### array-016 定数配列の要素にローカル変数を使わない
- 主根拠：`[array.constant]`
- 補助根拠：`[const.expression]`
- 結果：コンパイルエラー `[array.constant]` 3:14

```suz
fn main() -> u8 {
    var value = 3
    fix a = .[value]
    return 0
}
```

### array-017 定数配列の長さに実行時の値を使わない
- 主根拠：`[array.constant]`
- 補助根拠：`[const.expression]`・`[array.layout]`
- 結果：コンパイルエラー `[array.constant]` 1:37

```suz
fn make(n: u32) -> .[i32] { return .[0; n] }
fn main() -> u8 { return 0 }
```

### array-018 静的に範囲外と分かる定数添字を拒否する
- 主根拠：`[array.constant-index]`
- 補助根拠：`[array.access-handler]`・`[const.evaluation]`
- 結果：コンパイルエラー `[array.constant-index]` 3:18

```suz
fix TABLE = .[1, 2]
fn main() -> u8 {
    fix a = TABLE.[2]
    return 0
}
```

### array-019 定数配列に非nilのポインタを含めない
- 主根拠：`[array.constant]`
- 補助根拠：`[const.expression]`
- 結果：コンパイルエラー `[array.constant]` 2:14

```suz
fn main() -> u8 {
    fix a = .[new (1 as i32)]
    return 0
}
```

### array-069 整数添字の定数配列にstatic fnを含める
- 主根拠：`[array.constant]`
- 補助根拠：`[fn.value-const]`、`[fn.value-call]`
- 結果：正常終了

```suz
fnshape Get() -> i32
fn one() -> i32 { return 1 }
fn two() -> i32 { return 2 }
fix FS: .[static fn Get] = .[fn one, fn two]
fn main() -> u8 {
    if FS.[0]() != 1 || FS.[1]() != 2 { return 1 }
    return 0
}
```

### array-020 読み取り専用の配列も比較しない
- 主根拠：`[array.value]`
- 補助根拠：`[array.readonly]`・`[expr.equality]`
- 結果：コンパイルエラー `[array.value]` 3:10

```suz
fn main() -> u8 {
    fix a = .[1, 2]
    if a == a { return 0 }
    return 1
}
```

### array-021 配列参照をログのテンプレートに埋め込まない
- 主根拠：`[symbol.interpolation]`
- 補助根拠：`[array.readonly]`
- 結果：コンパイルエラー `[symbol.interpolation]` 3:19

```suz
fn main() -> u8 {
    fix a: .[i32] = .[1]
    log info "v=${a}"
    return 0
}
```

### array-022 トップレベルの定数配列もtextbufに埋め込まない
- 主根拠：`[symbol.interpolation]`
- 補助根拠：`[array.readonly]`
- 結果：コンパイルエラー `[symbol.interpolation]` 3:32

```suz
fix A: .[i32] = .[1]
fn main() -> u8 {
    fix message = textbuf "v=${A}"
    return 0
}
```

### array-023 配列参照へのポインタで内側の権限を変換しない
- 主根拠：`[array.readonly-conversion]`
- 補助根拠：`[type.expected]`・`[array.type]`
- 結果：コンパイルエラー `[type.expected]` 4:25

```suz
fn main() -> u8 {
    fix a: #[i32] = new #[1]
    fix q: mut #[i32] = new a
    fix p: mut .[i32] = q
    return 0
}
```

### array-024 配列参照を含む構造体も比較しない
- 主根拠：`[array.value]`
- 補助根拠：`[expr.equality]`
- 結果：コンパイルエラー `[array.value]` 5:10

```suz
struct Holder { values: .[i32] }
fn main() -> u8 {
    fix a = Holder { values: .[1] }
    fix b = Holder { values: .[1] }
    if a == b { return 1 }
    return 0
}
```

### array-107 配列参照とスカラーの順序比較はarray.valueで拒否される
- 主根拠：`[array.value]`
- 補助根拠：`[expr.comparison-chain]`・`[arith.compare]`
- 結果：コンパイルエラー `[array.value]` 3:15

```suz
fn main() -> u8 {
    fix a = new #[1, 2]
    fix m = a < 1.5
    return 0
}
```

### array-108 配列参照とスカラーの等値比較はarray.valueで拒否される
- 主根拠：`[array.value]`
- 補助根拠：`[expr.equality]`・`[arith.compare]`
- 結果：コンパイルエラー `[array.value]` 3:15

```suz
fn main() -> u8 {
    fix a = new #[1, 2]
    fix m = a == 1
    return 0
}
```

### array-026 書き込み可能な配列を読み取り専用の参照として使う
- 主根拠：`[array.readonly-conversion]`
- 補助根拠：`[array.type]`・`[array.readonly]`・`[array.constant]`・`[array.index]`・`[array.dimension-bind]`・`[array.bounds-proof]`・`[arena.let]`・`[branch.expression]`・`[generic.inference]`・`[struct.field]`・`[array.new]`
- 結果：正常終了

```suz
struct Hold { values: .[i32] }
fn first[T](a: .[T]) -> T { return a.[0] or panic "empty" }
fn view(a: #[i32]) -> .[i32] { return a }
fn sum(a: .[i32; n]) -> i32 {
    var total: i32 = 0
    for i in 0..<n { total += a.[i] }
    return total
}
fn main() -> u8 {
    fix a: #[i32] = new #[2, 4]
    var read: .[i32] = a
    fix holder = Hold { values: a }
    if sum(.[2 as i32, 4]) != sum(a) { return 1 }
    if first(a) != 2 { return 2 }
    a#[0] = 8
    if first(read) != 8 || first(holder.values) != 8 { return 3 }
    read = new #[3, 5]
    if sum(read) != 8 { return 4 }
    fix selected = ifexpr true { a } else { .[1 as i32] }
    if sum(selected) != 12 { return 5 }
    if sum(view(a)) != 12 { return 6 }
    fix maybe: #[i32]? = a
    let present = maybe { if sum(present) != 12 { return 7 } }
    fix empty: .[i32] = new #[]
    if len empty != 0 { return 8 }
    return 0
}
```

### array-027 多次元配列の変換と切り出しで実体を共有する
- 主根拠：`[array.readonly-conversion]`
- 補助根拠：`[array.type]`・`[array.index]`・`[array.slice]`・`[array.iteration]`・`[array.access-handler]`
- 結果：正常終了

```suz
fn view(a: #[i32; _, _]) -> .[i32; _, _] { return a }
fn main() -> u8 {
    fix a: #[i32; _, _] = new #[[1, 2], [3, 4]]
    fix read = view(a)
    fix row = read.[1] or panic "row"
    fix sliced = row.[0:2] or panic "slice"
    a.[1]#[0] = 9
    var total: i32 = 0
    rev value in sliced { total += value }
    if total != 13 { return 1 }
    fix direct: .[i32; _, _] = new #[[5, 6]]
    if direct.[0].[1] != 6 { return 2 }
    return 0
}
```

### array-030 読み取り専用の配列を書き込み可能に戻さない
- 主根拠：`[array.readonly-conversion]`
- 補助根拠：`[type.expected]`・`[array.type]`
- 結果：コンパイルエラー `[type.expected]` 3:21

```suz
fn main() -> u8 {
    fix a: .[i32] = .[1]
    fix b: #[i32] = a
    return 0
}
```

### array-031 配列の要素の数値型を暗黙に変えない
- 主根拠：`[array.readonly-conversion]`
- 補助根拠：`[type.expected]`・`[array.type]`
- 結果：コンパイルエラー `[type.expected]` 3:21

```suz
fn main() -> u8 {
    fix a: #[i32] = new #[1]
    fix b: .[i64] = a
    return 0
}
```

### array-032 配列の次元数を暗黙に変えない
- 主根拠：`[array.readonly-conversion]`
- 補助根拠：`[type.expected]`・`[array.type]`
- 結果：コンパイルエラー `[type.expected]` 3:21

```suz
fn main() -> u8 {
    fix a: #[i32; _, _] = new #[[1]]
    fix b: .[i32] = a
    return 0
}
```

### array-033 nil許容の配列参照を直接読み取り専用にしない
- 主根拠：`[array.readonly-conversion]`
- 補助根拠：`[type.expected]`・`[array.type]`・`[arena.let]`
- 結果：コンパイルエラー `[type.expected]` 3:21

```suz
fn main() -> u8 {
    fix a: #[i32]? = new #[1]
    fix b: .[i32] = a
    return 0
}
```

### array-034 配列要素の参照型を再帰的に変換しない
- 主根拠：`[array.readonly-conversion]`
- 補助根拠：`[type.expected]`・`[array.type]`
- 結果：コンパイルエラー `[type.expected]` 4:24

```suz
fn main() -> u8 {
    fix inner: #[i32] = new #[1]
    fix outer = new #[inner]
    fix a: .[.[i32]] = outer
    return 0
}
```

### array-035 mut? #[T] と mut #[T]? を区別する
- 主根拠：`[array.type]`
- 補助根拠：`[arena.let]`
- 結果：正常終了

```suz
fn main() -> u8 {
    arena {
        fix arr: #[i32] = new #[10, 20]
        fix outer_nil: mut? #[i32] = new arr
        let p = outer_nil {
            (.p)#[0] = 30
        } else {
            return 1
        }
        fix opt: #[i32]? = nil
        fix inner_nil: mut #[i32]? = new opt
        #inner_nil = arr
        let q = .inner_nil {
            if (q.[0] or 0) != 30 { return 2 }
        } else {
            return 3
        }
    }
    return 0
}
```

### array-036 実行時の値から読み取り専用配列をarenaへ確保する
- 主根拠：`[array.new]`
- 補助根拠：`[array.readonly]`
- 結果：正常終了

```suz
fn build(x: i32) -> i32 {
    arena {
        fix a: .[i32] = new .[x, x + 1, x + 2]
        return a.[2]
    }
}
fn main() -> u8 {
    if build(5) != 7 { return 1 }
    return 0
}
```

### array-038 nil許容の読み取り専用配列をnilとletで扱う
- 主根拠：`[array.type]`
- 補助根拠：`[arena.nil]`・`[arena.let]`
- 結果：正常終了

```suz
fn first_or_zero(a: .[i32]?) -> i32 {
    let v = a {
        return v.[0] or 0
    } else {
        return 0
    }
}
fn main() -> u8 {
    fix empty: .[i32]? = nil
    fix nums: .[i32]? = .[9, 10]
    if first_or_zero(empty) != 0 { return 1 }
    if first_or_zero(nums) != 9 { return 2 }
    return 0
}
```

### array-039 未確認のnil許容読み取り専用配列へ直接アクセスできない
- 主根拠：`[array.value]`
- 結果：コンパイルエラー `[array.value]` 3:14

```suz
fn main() -> u8 {
    fix a: .[i32]? = .[1, 2]
    fix x = a.[0] or 0
    return 0
}
```

### array-040 要素として書いた定数配列は行ではなく値として扱う
- 主根拠：`[array.enumeration]`
- 補助根拠：`[array.constant]`
- 結果：正常終了

```suz
fix JAGGED: .[.[i32]] = .[.[1, 2], .[3]]
fn main() -> u8 {
    arena {
        fix a: #[.[i32]] = new #[.[1, 2], .[3, 4]]
        if (a.[0].[1] or 0) != 2 || len JAGGED.[0] != 2 || len JAGGED.[1] != 1 { return 1 }
    }
    return 0
}
```

### array-041 配列初期化の同じ段に値と行を混在させない
- 主根拠：`[array.enumeration]`
- 結果：コンパイルエラー `[array.enumeration]` 2:18

```suz
fn main() -> u8 {
    fix a = new #[.[1, 2], [3, 4]]
    return 0
}
```

### array-070 多次元配列の内側の行に列挙子キーを書けない
- 主根拠：`[array.enumeration]`
- 補助根拠：`[array.enum-type]`、`[array.enum-init]`
- 結果：コンパイルエラー `[array.enumeration]` 3:16

```suz
enum Wave: u8 { Saw, Square }
fn main() -> u8 {
    fix a = .[[Wave.Square: 10, Wave.Saw: 20]]
    return 0
}
```

### array-042 newを付けない#[...]は認めない
- 主根拠：`[array.new]`
- 結果：コンパイルエラー `[array.new]` 2:13

```suz
fn main() -> u8 {
    fix a = #[1, 2]
    return 0
}
```

### array-043 #[T]と#[T]?を.[T]?へ暗黙変換する
- 主根拠：`[array.readonly-conversion]`
- 補助根拠：`[arena.let]`
- 結果：正常終了

```suz
fn main() -> u8 {
    arena {
        fix rw: #[i32] = new #[7, 8]
        fix opt: #[i32]? = rw
        fix a: .[i32]? = rw
        fix b: .[i32]? = opt
        let x = a {
            let y = b {
                if x.[0] != 7 || y.[1] != 8 { return 1 }
            } else { return 2 }
        } else { return 3 }
    }
    return 0
}
```

### array-044 #[T]?と.[T]の分岐を.[T]?へ揃える
- 主根拠：`[arena.ptr-join]`
- 補助根拠：`[array.readonly-conversion]`
- 結果：正常終了

```suz
fn main() -> u8 {
    arena {
        fix rw: #[i32] = new #[1, 2]
        fix opt: #[i32]? = rw
        fix ro: .[i32] = .[3, 4]
        fix joined = ifexpr false { opt } else { ro }
        let v = joined {
            if (v.[0] or 0) != 3 { return 1 }
        } else { return 2 }
    }
    return 0
}
```

### array-046 定数配列の要素に対する定数範囲外添字はorなしを拒否する
- 主根拠：`[array.constant-index]`
- 補助根拠：`[array.bounds-proof]`
- 結果：コンパイルエラー `[array.constant-index]` 3:28

```suz
fix JAGGED: .[.[i32]] = .[.[1, 2], .[3]]
fn main() -> u8 {
    fix x: i32 = JAGGED.[1].[5]
    return 0
}
```

### array-047 定数配列の要素に対する定数範囲外添字はorで代替値を使う
- 主根拠：`[array.constant-index]`
- 結果：正常終了

```suz
fix JAGGED: .[.[i32]] = .[.[1, 2], .[3]]
fn main() -> u8 {
    fix x: i32 = JAGGED.[1].[5] or 9
    if x != 9 { return 1 }
    return 0
}
```

### array-050 nil許容の参照とnilをトップレベル定数と定数配列で使う
- 主根拠：`[const.expression]`
- 補助根拠：`[array.constant]`・`[arena.ptr-conversion]`・`[array.readonly-conversion]`・`[arena.nil]`
- 結果：正常終了

```suz
struct Opt { xs: .[i32]? }
struct Node { value: i32, next: mut? Node }
fix A: .[i32] = .[1, 2]
fix N1: .[i32]? = .[1, 2]
fix N2: .[i32]? = nil
fix N3: .[i32]? = A
fix P: mut? i32 = nil
fix R: ref? i32 = P
fix W: #[i32]? = nil
fix O1 = Opt { xs: .[1, 2] }
fix O2 = Opt { xs: nil }
fix O3 = Opt { xs: A }
fix EMPTY = Node { value: 7, next: nil }
fix TABLE: .[.[i32]?] = .[.[3, 4], nil]
fix OPTS: .[Opt] = .[O1, O2]
fix NODES: .[Node] = .[EMPTY]

fn main() -> u8 {
    let a = N1 { if a.[1] != 2 { return 1 } } else { return 2 }
    let a = N2 { return 3 }
    let a = N3 { if a.[0] != 1 { return 4 } } else { return 5 }
    let p = P { return 6 }
    let r = R { return 7 }
    let w = W { return 8 }
    let a = O1.xs { if a.[1] != 2 { return 9 } } else { return 10 }
    let a = O2.xs { return 11 }
    let a = O3.xs { if a.[0] != 1 { return 12 } } else { return 13 }
    if EMPTY.value != 7 { return 14 }
    let n = EMPTY.next { return 15 }
    let a = TABLE.[0] { if a.[1] != 4 { return 16 } } else { return 17 }
    let a = TABLE.[1] { return 18 }
    let a = OPTS.[0].xs { if a.[0] != 1 { return 19 } } else { return 20 }
    let a = OPTS.[1].xs { return 21 }
    if NODES.[0].value != 7 { return 22 }
    let n = NODES.[0].next { return 23 }
    return 0
}
```
