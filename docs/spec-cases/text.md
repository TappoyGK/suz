# text

[目次と書式](../spec-cases.md)

### text-001 関数から返したtextを構造体へ保存してコピーする

主根拠：`[text.copy]`。

関数の復帰、元変数への再代入、構造体フィールドの更新後もコピー先の内容が変わらない。

**ソース：**

```suz
struct Message {
    body: text,
}
fn make(name: text) -> text {
    return `Hello, ${name}`
}
fn main() -> u8 {
    var name = `Suz`
    var first = Message { body: make(name) }
    fix copied = first
    name = `World`
    first.body = make(name)
    if copied.body == `Hello, Suz` && first.body == `Hello, World` { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-002 異なる生成経路の同値と空文字列を比較する

主根拠：`[text.compare]`。

これは比較結果の確認である。実体の共有そのものはランタイムを直接検証するテストで確認する。

**ソース：**

```suz
fn identity(value: text) -> text { return value }
fn main() -> u8 {
    fix name = identity(`Suz`)
    fix generated = `${name}`
    fix empty = identity(``)
    if name == generated && generated != `suz` && empty == `` { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-003 Unicodeの辞書順と接頭辞を比較する

主根拠：`[text.order]`。

「é」はU+00E9、分解形はU+0065とU+0301。「𐀀」はU+10000、「」はU+E000である。UTF-16のコード単位順との取り違えとUnicode正規化を検出する。

**ソース：**

```suz
fn main() -> u8 {
    fix composed = `é`
    fix decomposed = `é`
    if not (`` < `a` && `a` < `aa` && `A` < `a` <= `a`) { return 1 }
    if not (`あ` < `い` && `𐀀` > ``) { return 2 }
    if composed == decomposed { return 3 }
    if not (`a\0b` > `a` && `\0` < `\t`) { return 4 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-004 textを加算演算で連結しようとする

主根拠：`[text.operations]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = `a` + `b`
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[text.operations]` | 2 | 21 |

### text-005 数値とboolをSuzの表記で読み取る

主根拠：`[text.cast-format]`。

**ソース：**

```suz
fn main() -> u8 {
    fix integer = `1_000` as u16 or 0
    fix minimum = `-128` as i8 or 0
    fix fraction = `1.2_5e+1` as f64 or 0.0
    fix integral = `1` as f32 or 0.0
    fix yes = `true` as bool or false
    fix no = `false` as bool or true
    if integer == 1000 && minimum == -128 && fraction == 12.5 && integral == 1.0 && yes && not no { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-006 数値とboolの入力全体を検査する

主根拠：`[text.cast-format]`。

**ソース：**

```suz
fn invalid(value: text) -> bool {
    return (value as u8 or 99) == 99
}
fn main() -> u8 {
    if not (invalid(``) && invalid(` 1`) && invalid(`1 `)) { return 1 }
    if not (invalid(`1\0`) && invalid(`1+2`) && invalid(`(1)`)) { return 2 }
    if not (invalid(`+1`) && invalid(`1__0`) && invalid(`1.0`)) { return 3 }
    if not (invalid(`0x10`) && invalid(`1//comment`)) { return 4 }
    if (`True` as bool or false) { return 5 }
    if (`1` as bool or false) { return 6 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-007 整数の上下限と浮動小数点の丸めを確認する

主根拠：`[text.cast-range]`。

**ソース：**

```suz
fn main() -> u8 {
    fix maximum = `18446744073709551615` as u64 or 0
    fix overflow = `18446744073709551616` as u64 or 7
    fix underflow = `-129` as i8 or 7
    fix unsigned = `-0` as u8 or 7
    fix rounded = `16777217` as f32 or 0.0
    fix too_large = `1.0e100` as f32 or 7.0
    fix tiny = `1.0e-1000` as f64 or 7.0
    fix negative_zero = `-0` as f64 or 7.0
    fix special = `nan` as f64 or 0.0
    fix infinity = `-inf` as f64 or 0.0
    if maximum != 18446744073709551615 || overflow != 7 || underflow != 7 || unsigned != 7 { return 1 }
    if rounded != 16777216.0 || too_large != 7.0 || tiny != 0.0 { return 2 }
    if 1.0 / negative_zero != -inf || special == special || infinity != -inf { return 3 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-008 成功する定数文字列のasにもorを要求する

主根拠：`[text.cast-handler]`。

**ソース：**

```suz
const VALUE: u8 = `1` as u8
fn main() -> u8 { return VALUE }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[text.cast-handler]` | 1 | 23 |

### text-009 桁数の大きな整数と負数をas%でラップする

主根拠：`[text.cast-wrap]`。

**ソース：**

```suz
fn main() -> u8 {
    fix large = `18446744073709551617` as% u8 or 7
    fix negative = `-1` as% u8 or 7
    fix signed = `255` as% i8 or 7
    fix invalid = `18446744073709551617x` as% u8 or 7
    fix fraction = `1.0` as% u8 or 7
    if large == 1 && negative == 255 && signed == -1 && invalid == 7 && fraction == 7 { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-010 成功する定数文字列のas%にもorを要求する

主根拠：`[text.cast-handler]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = `256` as% u8
    return value
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[text.cast-handler]` | 2 | 23 |

### text-011 boolへのas%を拒否する

主根拠：`[text.cast-wrap]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = `true` as% bool or false
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[text.cast-wrap]` | 2 | 24 |

### text-012 text変換の評価回数と代替値の遅延評価

主根拠：`[text.cast-handler]`。

**ソース：**

```suz
fn source(value: text) -> text {
    log info `source`
    return value
}
fn fallback() -> u8 {
    log info `fallback`
    return 7
}
fn main() -> u8 {
    fix first = source(`1`) as u8 or fallback()
    fix second = source(`bad`) as% u8 or fallback()
    if first == 1 && second == 7 { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info text-012.suz:2: source
info text-012.suz:2: source
info text-012.suz:6: fallback
```

### text-013 text変換のor returnで呼び出し元に戻る

主根拠：`[text.cast-handler]`。

**ソース：**

```suz
fn parse(value: text) -> u8 {
    fix result = value as u8 or return 7
    return result
}
fn main() -> u8 {
    if parse(`2`) == 2 && parse(`bad`) == 7 { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-014 テンプレートの埋め込みにor returnを置く

主根拠：`[expr.or-return-position]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = `${`bad` as u8 or return 1}`
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.or-return-position]` | 2 | 35 |

### text-015 構造体へ入れ子の値とtextと列挙子を読み取る

主根拠：`[text.cast-struct]`。

**ソース：**

```suz
enum Mode: u8 { Fast, Slow }
struct Point {
    x: i32,
    y: i32,
}
struct Item {
    name: text,
    mode: Mode,
    point: Point,
}
fn main() -> u8 {
    fix input = `Item { point: Point { y: 2, x: 1 }, mode: Mode.Fast, name: \`Suz\` }`
    fix item = input as Item or return 1
    if item.name == `Suz` && item.mode == Mode.Fast && item.point.x == 1 && item.point.y == 2 { return 0 }
    return 2
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-016 構造体の型名とフィールドの過不足を検査する

主根拠：`[text.cast-struct]`。

**ソース：**

```suz
struct Point {
    x: u8,
    y: u8,
}
fn invalid(value: text) -> bool {
    fix point = value as Point or Point { x: 99, y: 99 }
    return point.x == 99 && point.y == 99
}
fn main() -> u8 {
    if not (invalid(`Other { x: 1, y: 2 }`) && invalid(`Point { x: 1 }`)) { return 1 }
    if not (invalid(`Point { x: 1, x: 2, y: 3 }`) && invalid(`Point { x: 1, y: 2, z: 3 }`)) { return 2 }
    if not (invalid(`Point { x: 256, y: 2 }`) && invalid(`Point { x: 1+2, y: 3 }`)) { return 3 }
    if not (invalid(`Point { x: 1, y: 2 }tail`) && invalid(`Point { x: 1, y: wrong }`)) { return 4 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-017 構造体内の文字列でエスケープを展開する

主根拠：`[text.cast-struct-text]`。

**ソース：**

```suz
struct Label {
    name: text,
}
fn main() -> u8 {
    fix input = `Label { name: \`a\\0b\` }`
    fix label = input as Label or return 1
    if label.name == `a\0b` && label.name != `a` { return 0 }
    return 2
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-018 変換入力内の不正エスケープと埋め込みを失敗にする

主根拠：`[text.cast-struct-text]`。

ソースのエスケープは合法である。復元された変換入力の不正をorが処理する。

**ソース：**

```suz
struct Label {
    name: text,
}
fn invalid(value: text) -> bool {
    fix label = value as Label or Label { name: `fallback` }
    return label.name == `fallback`
}
fn main() -> u8 {
    if not invalid(`Label { name: \`\\q\` }`) { return 1 }
    if not invalid(`Label { name: \`\${1}\` }`) { return 2 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-019 構造体入力の行数に応じて末尾カンマを検査する

主根拠：`[text.cast-struct-layout]`。

**ソース：**

```suz
struct Point {
    x: u8,
    y: u8,
}
fn valid(value: text) -> bool {
    fix point = value as Point or return false
    return point.x == 1 && point.y == 2
}
fn main() -> u8 {
    if not valid(`Point { x: 1, y: 2 }`) { return 1 }
    if not valid(`Point {\nx: 1,\ny: 2,\n}`) { return 2 }
    if not valid(`Point {\r\nx: 1,\r\ny: 2,\r\n}`) { return 3 }
    if valid(`Point { x: 1, y: 2, }`) { return 4 }
    if valid(`Point {\nx: 1,\ny: 2\n}`) { return 5 }
    if valid(`Point { x: 1\n, y: 2,\n}`) { return 6 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-020 constでテンプレートと型変換を評価する

主根拠：`[const.text-cast]`。

**ソース：**

```suz
struct Label {
    name: text,
}
const PRODUCT = `Suz`
const MAJOR: u8 = 1
const LABEL = `${PRODUCT} ${MAJOR}`
const COUNT: u8 = `256` as u8 or 10
const WRAPPED = `257` as% u8 or 10
const OBJECT = `Label { name: \`Suz\` }` as Label or Label { name: `` }
fn main() -> u8 {
    if LABEL == `Suz 1` && COUNT == 10 && WRAPPED == 1 && OBJECT.name == PRODUCT { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-021 textをmatchとmatchexprで分岐する

主根拠：`[text.match]`。

**ソース：**

```suz
fn classify(value: text) -> u8 {
    return matchexpr value {
        `ready` => 1
        `busy` => 2
        _ => 3
    }
}
fn main() -> u8 {
    var result: u8 = 0
    match `busy` {
        `ready` => { result = 1 }
        `busy` => { result = 2 }
        _ => { result = 3 }
    }
    if result == 2 && classify(`ready`) == 1 && classify(`other`) == 3 { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-022 内容が等しいtextパターンの重複を拒否する

主根拠：`[match.pattern]`。

**ソース：**

```suz
const NAME = `ready`
fn main() -> u8 {
    match `ready` {
        NAME => { return 0 }
        `${NAME}` => { return 1 }
        _ => { return 2 }
    }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[match.pattern]` | 5 | 9 |

### text-023 textのmatchに最後のワイルドカードを要求する

主根拠：`[match.exhaustive]`。

**ソース：**

```suz
fn main() -> u8 {
    match `ready` {
        `ready` => { return 0 }
    }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[match.exhaustive]` | 2 | 5 |

### text-024 text生成でも埋め込みを左から右へ1回ずつ評価する

主根拠：`[text.interpolation]`。

**ソース：**

```suz
fn piece(value: u8) -> text {
    log info `${value}`
    return `${value}`
}
fn main() -> u8 {
    fix value = `${piece(1)}:${piece(2)}`
    if value == `1:2` { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info text-024.suz:2: 1
info text-024.suz:2: 2
```

### text-025 textをログへ埋め込む

主根拠：`[log.temporary-text]`。

**ソース：**

```suz
fn main() -> u8 {
    fix message = `Hello`
    log info `${message}`
    log info `count: ${1}`
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info text-025.suz:3: Hello
info text-025.suz:4: count: 1
```
