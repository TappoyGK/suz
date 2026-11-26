# 警告と診断設定

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

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 3 | 0 |
| 警告 | 正常終了 | 3 | 1 |
| 強制 | コンパイルエラー | — | 0 |

対象は+の1箇所。通常時の終了コードは3。

**警告：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 警告 | `[diagnostic.arithmetic]` | 2 | 23 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 強制 | `[diagnostic.arithmetic]` | 2 | 23 |

### diag-002 orは算術式全体の失敗を処理する

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

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 4 | 0 |
| 警告 | 正常終了 | 4 | 0 |
| 強制 | 正常終了 | 4 | 0 |

算術式全体にorがあるため、警告設定でも警告せず、強制設定でも受理する。

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

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 1 | 0 |
| 警告 | 正常終了 | 1 | 1 |
| 強制 | コンパイルエラー | — | 0 |

対象は+=の1箇所。通常時の終了コードは1。

**警告：**

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

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |
| 警告 | 正常終了 | 0 | 1 |
| 強制 | コンパイルエラー | — | 0 |

対象は+の1箇所。分岐が実行されなくても診断対象になる。通常時の終了コードは0。

**警告：**

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

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |
| 警告 | 正常終了 | 0 | 2 |
| 強制 | コンパイルエラー | — | 0 |

i8の最小値を作る単項マイナスと、return後の実行されない単項マイナスの両方が対象となる。警告設定では2件の警告を出して実行でき、強制設定では最初の演算をエラーにする。

**警告：**

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

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 4 | 0 |
| 警告 | 正常終了 | 4 | 0 |
| 強制 | 正常終了 | 4 | 0 |

### diag-007 ラップ演算は警告対象に含まれない

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

| 設定 | 結果 | 終了コード | 警告数 |
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

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 11 | 0 |
| 警告 | 正常終了 | 11 | 1 |
| 強制 | コンパイルエラー | — | 0 |

除算には対応するorがあり、診断対象は+=の1箇所。通常時の終了コードは11。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 強制 | `[diagnostic.arithmetic]` | 4 | 11 |

**警告：**

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

| 設定 | 結果 | 終了コード | 警告数 |
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

### diag-015 飽和左シフトは失敗しうる右オペランドだけを診断対象にする

主根拠：`[diagnostic.arithmetic-target]`。

補助根拠：`[expr.sat-shift]`。

**ソース：**

```suz
fn main() -> u8 {
    fix u: u8 = 1
    fix s: i32 = 1
    fix a: i8 = (1 as i8) <<| u
    fix b: i8 = (1 as i8) <<| s
    return ifexpr a == 2 && b == 2 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |
| 警告 | 正常終了 | 0 | 1 |
| 強制 | コンパイルエラー | — | 0 |

**警告：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 警告 | `[diagnostic.arithmetic]` | 5 | 27 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 強制 | `[diagnostic.arithmetic]` | 5 | 27 |

### diag-013 範囲だけで決まる比較を通常設定で警告する

主根拠：`[diagnostic.constant-comparison]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value: u8 = 7
    fix result = value + 1 < 1000
    return ifexpr result { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 1 |

左側の範囲は1以上256以下なので常に真となる。整数演算の警告オプションを指定しなくても、比較演算子に警告する。

**警告：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[diagnostic.constant-comparison]` | 3 | 28 |

### diag-014 浮動小数点演算に先行する整数除算を警告する

主根拠：`[diagnostic.integer-division-to-float]`。

**ソース：**

```suz
fn main() -> u8 {
    fix frame: i32 = 3
    fix frames: i32 = 10
    fix truncated = frame / frames * 2.0
    fix fraction = frame * 2.0 / frames
    fix intended = frame / frames * 2.0 allow [diagnostic.integer-division-to-float: whole frames]
    return ifexpr truncated == 0.0 && fraction == 0.6 && intended == 0.0 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 1 |

truncatedでは先に整数の商0を求めるため警告する。fractionは浮動小数点除算となる。intendedは理由付きで警告を抑制する。

**警告：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[diagnostic.integer-division-to-float]` | 4 | 27 |
### diag-010 allowは警告を抑え、強制設定のエラーは抑えない

主根拠：`[allow.clause]`。

**ソース：**

```suz
fn main() -> u8 {
    fix count: u8 = 3
    fix next = count + 1 allow [diagnostic.arithmetic: count is 3]
    return next
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 4 | 0 |
| 警告 | 正常終了 | 4 | 0 |
| 強制 | コンパイルエラー | — | 0 |

演算は同じ結果を返し、警告だけを抑える。強制設定では演算子の位置を診断する。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 強制 | `[diagnostic.arithmetic]` | 3 | 22 |

### diag-011 ブロックを含む式の末尾にはallowを書けない

主根拠：`[allow.contains-block]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = (expr { 1 + 2 }) allow [diagnostic.arithmetic: small]
    return value
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

丸括弧で囲んでも、exprのブロックを含む文の末尾には指定できない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[allow.contains-block]` | 2 | 34 |

### diag-012 警告設定を省略しても不要なallowを拒否する

主根拠：`[allow.unused]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value: u8 = 1 allow [diagnostic.arithmetic: no operation]
    return value
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

警告対象の演算がないため、指定したallowを診断する。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[allow.unused]` | 2 | 29 |
