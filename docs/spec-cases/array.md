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
