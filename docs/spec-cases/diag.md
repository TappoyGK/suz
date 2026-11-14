# 11. 整数演算の診断設定

[目次と書式](../spec-cases.md)

警告設定は `--warn-unhandled-arithmetic`、強制設定は `--deny-unhandled-arithmetic`。

### diag-001 安全な値でもorなしの整数演算を診断

主根拠：`[diagnostic.arithmetic]`。

補助根拠：`[diagnostic.arithmetic-target]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value: u8 = 1 + 2
    return value
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 3 | 0 |
| 警告 | 正常終了 | 3 | 1 |
| 強制 | コンパイルエラー | — | 0 |

対象は+の1箇所。通常時の終了コードは3。

**整数演算の警告：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 警告 | `[diagnostic.arithmetic]` | 2 | 23 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 強制 | `[diagnostic.arithmetic]` | 2 | 23 |

### diag-002 最も外側のorだけでは内側の演算が残る

主根拠：`[diagnostic.arithmetic]`。

補助根拠：`[diagnostic.arithmetic-target]`。

**ソース：**

```suz
fn main() -> u8 {
    fix a: u8 = 1
    fix value = ((a + 1) * 2) or 0
    return value
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 4 | 0 |
| 警告 | 正常終了 | 4 | 1 |
| 強制 | コンパイルエラー | — | 0 |

対象は内側の+の1箇所。通常時の終了コードは4。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 強制 | `[diagnostic.arithmetic]` | 3 | 21 |

**整数演算の警告：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 警告 | `[diagnostic.arithmetic]` | 3 | 21 |

### diag-003 複合代入も診断対象

主根拠：`[diagnostic.arithmetic]`。

補助根拠：`[diagnostic.arithmetic-target]`。

**ソース：**

```suz
fn main() -> u8 {
    var count: u8 = 0
    count += 1
    return count
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 1 | 0 |
| 警告 | 正常終了 | 1 | 1 |
| 強制 | コンパイルエラー | — | 0 |

対象は+=の1箇所。通常時の終了コードは1。

**整数演算の警告：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 警告 | `[diagnostic.arithmetic]` | 3 | 11 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 強制 | `[diagnostic.arithmetic]` | 3 | 11 |

### diag-004 実行されない分岐の整数演算も診断する

主根拠：`[diagnostic.arithmetic]`。

補助根拠：`[diagnostic.arithmetic-target]`。

**ソース：**

```suz
fn main() -> u8 {
    fix top: u8 = 255
    if false {
        fix result = top + 1
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |
| 警告 | 正常終了 | 0 | 1 |
| 強制 | コンパイルエラー | — | 0 |

対象は+の1箇所。分岐が実行されなくても診断対象になる。通常時の終了コードは0。

**整数演算の警告：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 警告 | `[diagnostic.arithmetic]` | 4 | 26 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 強制 | `[diagnostic.arithmetic]` | 4 | 26 |

### diag-005 負の整数リテラルとreturn後の演算を診断する

主根拠：`[diagnostic.arithmetic]`。

補助根拠：`[diagnostic.arithmetic-target]`。

**ソース：**

```suz
fn main() -> u8 {
    fix small: i8 = -128
    return 0
    fix other = -1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |
| 警告 | 正常終了 | 0 | 2 |
| 強制 | コンパイルエラー | — | 0 |

i8の最小値を作る単項マイナスと、return後の実行されない単項マイナスの両方が対象となる。警告設定では2件の警告を出して実行でき、強制設定では最初の演算をエラーにする。

**整数演算の警告：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 警告 | `[diagnostic.arithmetic]` | 2 | 21 |
| 警告 | `[diagnostic.arithmetic]` | 4 | 17 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 強制 | `[diagnostic.arithmetic]` | 2 | 21 |

### diag-006 全演算に失敗処理を付ける

主根拠：`[diagnostic.arithmetic-target]`。

補助根拠：`[expr.arithmetic-scope]`。

**ソース：**

```suz
fn main() -> u8 {
    fix a: u8 = 1
    fix value = (((a + 1) or 0) * 2) or 0
    return value
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 4 | 0 |
| 警告 | 正常終了 | 4 | 0 |
| 強制 | 正常終了 | 4 | 0 |

### diag-007 ラップ演算は整数演算の警告対象に含まれない

主根拠：`[diagnostic.arithmetic-target]`。

補助根拠：`[expr.wrap]`、`[expr.as-trunc]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value: u8 = 255 +% 1
    fix low = 257 as% u8
    return value +% low
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 1 | 0 |
| 警告 | 正常終了 | 1 | 0 |
| 強制 | 正常終了 | 1 | 0 |

### diag-008 右辺の失敗処理とは別に複合代入を診断する

主根拠：`[diagnostic.arithmetic-target]`。

補助根拠：`[expr.arithmetic-scope]`。

**ソース：**

```suz
fn main() -> u8 {
    var count: u8 = 10
    fix divisor: u8 = 1
    count += (1 / divisor) or 0
    return count
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 11 | 0 |
| 警告 | 正常終了 | 11 | 1 |
| 強制 | コンパイルエラー | — | 0 |

除算には対応するorがあり、診断対象は+=の1箇所。通常時の終了コードは11。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 強制 | `[diagnostic.arithmetic]` | 4 | 11 |

**整数演算の警告：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 警告 | `[diagnostic.arithmetic]` | 4 | 11 |

### diag-009 キャストのor必須ルールは診断設定で変わらない

主根拠：`[diagnostic.arithmetic-target]`。

補助根拠：`[expr.cast-handler]`。

**ソース：**

```suz
fn main() -> u8 {
    fix raw: i64 = 1
    fix value = raw as u8
    return value
}
```
**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |
| 警告 | コンパイルエラー | — | 0 |
| 強制 | コンパイルエラー | — | 0 |

rawは定数式ではなく、i64からu8への変換にはorが必要。警告設定でもエラーを警告には変更しない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.cast-handler]` | 3 | 21 |
| 警告 | `[expr.cast-handler]` | 3 | 21 |
| 強制 | `[expr.cast-handler]` | 3 | 21 |
