# ログ

[目次と書式](../spec-cases.md)

### log-001 テンプレートを変数に保存

主根拠：`[symbol.template]`。

**ソース：**

```suz
fn main() -> u8 {
    fix message = "hello"
    log info "${message}"
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

テンプレートをtextの値として保存し、ログへ埋め込む。

**標準エラー：**

```text
info log-001.suz:3: hello
```

### log-002 テンプレートをソース上で複数行にする

主根拠：`[symbol.template-line]`。

**ソース：**

```suz
fn main() -> u8 {
    log info "first
second"
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

開く二重引用符と閉じる二重引用符は同じ行に書く必要がある。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[symbol.template-line]` | 2 | 20 |

### log-014 テンプレート本文にバッククォートとエスケープされた二重引用符を含める

主根拠：`[symbol.template-text]`。

補助根拠：`[symbol.escape]`、`[log.output]`。

**ソース：**

```suz
fn main() -> u8 {
    log info "a`b`c \"q\""
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info log-014.suz:2: a`b`c "q"
```

### log-003 ログに構造体を埋め込む

主根拠：`[symbol.interpolation]`。

**ソース：**

```suz
struct Point { x: i32 }
fn main() -> u8 {
    fix p = Point { x: 1 }
    log info "${p}"
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

構造体は埋め込める型に含まれない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[symbol.interpolation]` | 4 | 17 |

### log-004 ログ内の関数呼び出しを禁止する

主根拠：`[output.embed-value]`。

**ソース：**

```suz
fn action() -> void {}

fn main() -> u8 {
    log info "${action()}"
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

戻り値の型によらず、出力の埋め込みに関数呼び出しは書けない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[output.embed-value]` | 4 | 17 |

### log-005 計算した値をログに渡す

主根拠：`[log.evaluation]`。

補助根拠：`[log.output]`。

**ソース：**

```suz
fn value() -> u8 {
    log info "inner"
    return 3
}

fn main() -> u8 {
    fix result = value()
    log info "outer: ${result}"
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

valueを先に呼び出して結果を束縛する。innerの行が完了した後、結果を使ったouterの行が続く。

**標準エラー：**

```text
info log-005.suz:2: inner
info log-005.suz:8: outer: 3
```

### log-006 ログの前の計算でパニックする

主根拠：`[log.evaluation]`。

補助根拠：`[rt.div-zero]`、`[rt.panic]`。

**ソース：**

```suz
fn divide(divisor: i32) -> i32 {
    log info "inner"
    return 1 / divisor
}

fn main() -> u8 {
    fix result = divide(0)
    log info "outer: ${result}"
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | パニック | 101 | 0 |

divideはinnerを出力した後、ゼロ除算でパニックする。後続のログ文には到達せず、接頭辞も本文も出力しない。

**パニック診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[rt.div-zero]` | 3 | 14 |

標準エラーは次の行で始まり、その後にlog-006.suzの3行目の除算でゼロ除算が起きたことを示すパニック診断が続く。

**パニック前の標準エラー：**

```text
info log-006.suz:2: inner
```

### log-007 定義されていないテンプレートのエスケープ

主根拠：`[symbol.escape]`。

**ソース：**

```suz
fn main() -> u8 {
    log info "invalid: \q"
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

\qは定義されているエスケープに含まれない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[symbol.escape]` | 2 | 24 |

### log-008 ログのエスケープと本文のコメント記号

主根拠：`[symbol.escape]`。

補助根拠：`[log.output]`、`[symbol.template-text]`。

**ソース：**

```suz
fn main() -> u8 {
    log info "literal: \${value} // text\nnext"
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info log-008.suz:2: literal: ${value} // text
next
```

### log-015 バッククォートのエスケープは未定義として拒否する

主根拠：`[symbol.escape]`。

**ソース：**

```suz
fn main() -> u8 {
    log info "invalid: \`"
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
| 通常 | `[symbol.escape]` | 2 | 24 |

### log-016 テンプレートのUnicodeエスケープ

主根拠：`[symbol.escape]`。

**ソース：**

```suz
fn main() -> u8 {
    fix s = "\u{41}\u{200B}\u{1F600}"
    if s != "A${\u{200B}}😀" { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### log-017 シングルクォートのエスケープはテンプレートで拒否する

主根拠：`[symbol.escape]`。

**ソース：**

```suz
fn main() -> u8 {
    log info "invalid: \'"
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
| 通常 | `[symbol.escape]` | 2 | 24 |

### log-018 テンプレートのUnicodeエスケープの範囲外を拒否する

主根拠：`[symbol.escape]`。

**ソース：**

```suz
fn main() -> u8 {
    log info "\u{D800}"
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
| 通常 | `[symbol.escape]` | 2 | 15 |

### log-009 bool・enum・整数のログ表示

主根拠：`[symbol.value-format]`。

補助根拠：`[log.output]`。

**ソース：**

```suz
enum Wave: u8 { Sine, Saw }

fn main() -> u8 {
    fix yes = true
    fix no = false
    fix count = 1_234
    fix negative = -7
    log info "${yes} ${no} ${Wave.Saw} ${count} ${negative}"
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

enumは型名を付けず列挙子名だけを表示する。整数の区切りのアンダースコアは表示しない。

**標準エラー：**

```text
info log-009.suz:8: true false Saw 1234 -7
```

### log-010 浮動小数点の通常表記と小さい値の指数表記

主根拠：`[symbol.float-format]`。

補助根拠：`[symbol.value-format]`。

**ソース：**

```suz
fn main() -> u8 {
    fix whole = 1.0
    fix fraction = 0.125
    fix tiny = 1.0e-8
    fix negative = -1.0e-8
    log info "${whole} ${fraction} ${tiny} ${negative}"
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

整数に見える値にも小数点を付ける。絶対値が1e-6未満の0でない値は指数表記にする。

**標準エラー：**

```text
info log-010.suz:6: 1.0 0.125 1.0e-8 -1.0e-8
```

### log-011 warnレベルと同名の変数

主根拠：`[log.level]`。

**ソース：**

```suz
fn main() -> u8 {
    fix warn: u8 = 7
    log warn "count: ${warn}"
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

logの直後ではwarnレベルを指定し、埋め込み式のwarnは変数を参照する。

**標準エラー：**

```text
warn log-011.suz:3: count: 7
```

### log-012 4レベルとerror出力後の続行

主根拠：`[log.level]`。

補助根拠：`[src.log-level-words]`、`[log.output]`。

**ソース：**

```suz
fn main() -> u8 {
    fix debug = 1
    fix info = 2
    fix warn = 3
    log debug "${debug}"
    log info "${info}"
    log warn "${warn}"
    log error "failed"
    return 7
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 7 | 0 |

既定の最低レベルはinfoなのでdebugの行は出力しない。errorレベルの出力後もreturnへ進む。errorは既存の予約語であり、変数名には使わない。

**標準エラー：**

```text
info log-012.suz:6: 2
warn log-012.suz:7: 3
error log-012.suz:8: failed
```

### log-013 panicの埋め込みでも計算を禁止する

主根拠：`[output.embed-value]`。

**ソース：**

```suz
fn main() -> u8 {
    fix count = 1
    panic "count: ${count + 1}"
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[output.embed-value]` | 3 | 27 |
