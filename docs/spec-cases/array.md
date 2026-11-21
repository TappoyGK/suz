# 配列

[目次と書式](../spec-cases.md)

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
