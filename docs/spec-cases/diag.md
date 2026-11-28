# 警告と診断設定

[目次と書式](../spec-cases.md)

警告設定は `--warn-unhandled-arithmetic`、強制設定は `--deny-unhandled-arithmetic`。

### diag-001 安全な値でもorなしの整数演算を診断
- 主根拠：`[diagnostic.arithmetic]`
- 補助根拠：`[diagnostic.arithmetic-target]`
- 結果（通常）：正常終了 3
- 結果（警告）：正常終了 3 / 警告：`[diagnostic.arithmetic]` 2:23
- 結果（強制）：コンパイルエラー `[diagnostic.arithmetic]` 2:23

対象は+の1箇所。通常時の終了コードは3。

```suz
fn main() -> u8 {
    fix value: u8 = 1 + 2
    return value
}
```

### diag-002 orは算術式全体の失敗を処理する
- 主根拠：`[diagnostic.arithmetic]`
- 補助根拠：`[diagnostic.arithmetic-target]`
- 結果（通常）：正常終了 4
- 結果（警告）：正常終了 4
- 結果（強制）：正常終了 4

算術式全体にorがあるため、警告設定でも警告せず、強制設定でも受理する。

```suz
fn main() -> u8 {
    fix a: u8 = 1
    fix value = ((a + 1) * 2) or 0
    return value
}
```

### diag-003 複合代入も診断対象
- 主根拠：`[diagnostic.arithmetic]`
- 補助根拠：`[diagnostic.arithmetic-target]`
- 結果（通常）：正常終了 1
- 結果（警告）：正常終了 1 / 警告：`[diagnostic.arithmetic]` 3:11
- 結果（強制）：コンパイルエラー `[diagnostic.arithmetic]` 3:11

対象は+=の1箇所。通常時の終了コードは1。

```suz
fn main() -> u8 {
    var count: u8 = 0
    count += 1
    return count
}
```

### diag-004 実行されない分岐の整数演算も診断する
- 主根拠：`[diagnostic.arithmetic]`
- 補助根拠：`[diagnostic.arithmetic-target]`
- 結果（通常）：正常終了
- 結果（警告）：正常終了 / 警告：`[diagnostic.arithmetic]` 4:26
- 結果（強制）：コンパイルエラー `[diagnostic.arithmetic]` 4:26

対象は+の1箇所。分岐が実行されなくても診断対象になる。通常時の終了コードは0。

```suz
fn main() -> u8 {
    fix top: u8 = 255
    if false {
        fix result = top + 1
    }
    return 0
}
```

### diag-005 負の整数リテラルとreturn後の演算を診断する
- 主根拠：`[diagnostic.arithmetic]`
- 補助根拠：`[diagnostic.arithmetic-target]`
- 結果（通常）：正常終了
- 結果（警告）：正常終了 / 警告：`[diagnostic.arithmetic]` 2:21, `[diagnostic.arithmetic]` 4:17
- 結果（強制）：コンパイルエラー `[diagnostic.arithmetic]` 2:21

i8の最小値を作る単項マイナスと、return後の実行されない単項マイナスの両方が対象となる。警告設定では2件の警告を出して実行でき、強制設定では最初の演算をエラーにする。

```suz
fn main() -> u8 {
    fix small: i8 = -128
    return 0
    fix other = -1
}
```

### diag-006 全演算に失敗処理を付ける
- 主根拠：`[diagnostic.arithmetic-target]`
- 補助根拠：`[expr.arithmetic-scope]`
- 結果（通常）：正常終了 4
- 結果（警告）：正常終了 4
- 結果（強制）：正常終了 4

```suz
fn main() -> u8 {
    fix a: u8 = 1
    fix value = (((a + 1) or 0) * 2) or 0
    return value
}
```

### diag-007 ラップ演算は警告対象に含まれない
- 主根拠：`[diagnostic.arithmetic-target]`
- 補助根拠：`[expr.wrap]`、`[expr.as-trunc]`
- 結果（通常）：正常終了 1
- 結果（警告）：正常終了 1
- 結果（強制）：正常終了 1

```suz
fn main() -> u8 {
    fix value: u8 = 255 +% 1
    fix low = 257 as% u8
    return value +% low
}
```

### diag-008 右辺の失敗処理とは別に複合代入を診断する
- 主根拠：`[diagnostic.arithmetic-target]`
- 補助根拠：`[expr.arithmetic-scope]`
- 結果（通常）：正常終了 11
- 結果（警告）：正常終了 11 / 警告：`[diagnostic.arithmetic]` 4:11
- 結果（強制）：コンパイルエラー `[diagnostic.arithmetic]` 4:11

除算には対応するorがあり、診断対象は+=の1箇所。通常時の終了コードは11。

```suz
fn main() -> u8 {
    var count: u8 = 10
    fix divisor: u8 = 1
    count += (1 / divisor) or 0
    return count
}
```

### diag-009 キャストのor必須ルールは診断設定で変わらない
- 主根拠：`[diagnostic.arithmetic-target]`
- 補助根拠：`[expr.cast-handler]`
- 結果（通常）：コンパイルエラー `[expr.cast-handler]` 3:21
- 結果（警告）：コンパイルエラー `[expr.cast-handler]` 3:21
- 結果（強制）：コンパイルエラー `[expr.cast-handler]` 3:21

rawは定数式ではなく、i64からu8への変換にはorが必要。警告設定でもエラーを警告には変更しない。

```suz
fn main() -> u8 {
    fix raw: i64 = 1
    fix value = raw as u8
    return value
}
```

### diag-015 飽和左シフトは失敗しうる右オペランドだけを診断対象にする
- 主根拠：`[diagnostic.arithmetic-target]`
- 補助根拠：`[expr.sat-shift]`
- 結果（通常）：正常終了
- 結果（警告）：正常終了 / 警告：`[diagnostic.arithmetic]` 5:27
- 結果（強制）：コンパイルエラー `[diagnostic.arithmetic]` 5:27

```suz
fn main() -> u8 {
    fix u: u8 = 1
    fix s: i32 = 1
    fix a: i8 = (1 as i8) <<| u
    fix b: i8 = (1 as i8) <<| s
    return ifexpr a == 2 && b == 2 { 0 } else { 1 }
}
```

### diag-013 範囲だけで決まる比較を通常設定で警告する
- 主根拠：`[diagnostic.constant-comparison]`
- 結果：正常終了 / 警告：`[diagnostic.constant-comparison]` 3:28

左側の範囲は1以上256以下なので常に真となる。整数演算の警告オプションを指定しなくても、比較演算子に警告する。

```suz
fn main() -> u8 {
    fix value: u8 = 7
    fix result = value + 1 < 1000
    return ifexpr result { 0 } else { 1 }
}
```

### diag-014 浮動小数点演算に先行する整数除算を警告する
- 主根拠：`[diagnostic.integer-division-to-float]`
- 結果：正常終了 / 警告：`[diagnostic.integer-division-to-float]` 4:27

truncatedでは先に整数の商0を求めるため警告する。fractionは浮動小数点除算となる。intendedは理由付きで警告を抑制する。

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

### diag-010 allowは警告を抑え、強制設定のエラーは抑えない
- 主根拠：`[allow.clause]`
- 結果（通常）：正常終了 4
- 結果（警告）：正常終了 4
- 結果（強制）：コンパイルエラー `[diagnostic.arithmetic]` 3:22

演算は同じ結果を返し、警告だけを抑える。強制設定では演算子の位置を診断する。

```suz
fn main() -> u8 {
    fix count: u8 = 3
    fix next = count + 1 allow [diagnostic.arithmetic: count is 3]
    return next
}
```

### diag-011 ブロックを含む式の末尾にはallowを書けない
- 主根拠：`[allow.contains-block]`
- 結果：コンパイルエラー `[allow.contains-block]` 2:34

丸括弧で囲んでも、exprのブロックを含む文の末尾には指定できない。

```suz
fn main() -> u8 {
    fix value = (expr { 1 + 2 }) allow [diagnostic.arithmetic: small]
    return value
}
```

### diag-012 警告設定を省略しても不要なallowを拒否する
- 主根拠：`[allow.unused]`
- 結果：コンパイルエラー `[allow.unused]` 2:29

警告対象の演算がないため、指定したallowを診断する。

```suz
fn main() -> u8 {
    fix value: u8 = 1 allow [diagnostic.arithmetic: no operation]
    return value
}
```
