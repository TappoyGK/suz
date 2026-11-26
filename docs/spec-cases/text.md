# symbolとtext

[目次と書式](../spec-cases.md)

### text-001 関数から返したsymbolを構造体へ保存してコピーする

主根拠：`[symbol.copy]`。

関数の復帰、元変数への再代入、構造体フィールドの更新後もコピー先の内容が変わらない。

**ソース：**

```suz
struct Message {
    body: symbol,
}
fn make(name: symbol) -> symbol {
    return "Hello, ${name}"
}
fn main() -> u8 {
    var name: symbol = "Suz"
    var first = Message { body: make(name) }
    fix copied = first
    name = "World"
    first#body = make(name)
    if copied.body == "Hello, Suz" && first.body == "Hello, World" { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-002 異なる生成経路の同値と空文字列を比較する

主根拠：`[symbol.compare]`。

これは比較結果の確認である。実体の共有そのものはランタイムを直接検証するテストで確認する。

**ソース：**

```suz
fn identity(value: symbol) -> symbol { return value }
fn main() -> u8 {
    fix name = identity("Suz")
    fix generated: symbol = "${name}"
    fix empty = identity("")
    if name == generated && generated != "suz" && empty == "" { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-003 Unicodeの辞書順と接頭辞を比較する

主根拠：`[symbol.order]`。

「é」はU+00E9、分解形はU+0065とU+0301。「𐀀」はU+10000、「」はU+E000である。UTF-16のコード単位順との取り違えとUnicode正規化を検出する。

**ソース：**

```suz
fn symbol_value(value: symbol) -> symbol { return value }
fn main() -> u8 {
    fix composed = symbol_value("é")
    fix decomposed = symbol_value("é")
    if not (symbol_value("") < symbol_value("a") && symbol_value("a") < symbol_value("aa") && symbol_value("A") < symbol_value("a") <= symbol_value("a")) { return 1 }
    if not (symbol_value("あ") < symbol_value("い") && symbol_value("𐀀") > symbol_value("")) { return 2 }
    if composed == decomposed { return 3 }
    if not (symbol_value("a\0b") > symbol_value("a") && symbol_value("\0") < symbol_value("\t")) { return 4 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-004 symbolを加算演算で連結しようとする

主根拠：`[symbol.operations]`。

**ソース：**

```suz
fn main() -> u8 {
    fix left: symbol = "a"
    fix right: symbol = "b"
    fix value = left + right
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
| 通常 | `[symbol.operations]` | 4 | 22 |

### text-026 symbolをtextとして渡し、分岐と構造体に保持する

主根拠：`[text.from-symbol]`。

**ソース：**

```suz
fix ORIGINAL: symbol = "abc"
struct Message { body: text }
fn keep(value: text) -> text { return value }
fn main() -> u8 {
    var original: symbol = "abc"
    fix saved = keep(original)
    original = "other"
    fix first = ifexpr true { ORIGINAL } else { saved }
    fix second = ifexpr false { saved } else { ORIGINAL }
    fix message = Message { body: first }
    if message.body != second { return 1 }
    if saved != "abc" { return 2 }
    if original != "other" { return 3 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-033 textをsymbolへ暗黙変換しない

主根拠：`[text.from-symbol]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value: symbol = "abc".[:]
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
| 通常 | `[type.expected]` | 2 | 30 |
### text-020 constでテンプレートと型変換を評価する

主根拠：`[const.symbol-cast]`。

**ソース：**

```suz
struct Label {
    name: symbol,
}
fix PRODUCT: symbol = "Suz"
fix MAJOR: u8 = 1
fix LABEL: symbol = "${PRODUCT} ${MAJOR}"
fix COUNT: u8 = "256" as u8 or 10
fix WRAPPED = "257" as% u8 or 10
fix OBJECT = "Label { name: \"Suz\" }" as Label or Label { name: "" }
fn main() -> u8 {
    if LABEL == "Suz 1" && COUNT == 10 && WRAPPED == 1 && OBJECT.name == PRODUCT { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-014 テンプレートの埋め込みにor returnを置く

主根拠：`[expr.or-return-position]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value: symbol = "${"bad" as u8 or return 1}"
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
| 通常 | `[expr.or-return-position]` | 2 | 43 |

### text-024 symbol生成でも埋め込みを左から右へ1回ずつ評価する

主根拠：`[symbol.interpolation]`。

**ソース：**

```suz
fn piece(value: u8) -> symbol {
    log info "${value}"
    return "${value}"
}
fn main() -> u8 {
    fix value: symbol = "${piece(1)}:${piece(2)}"
    if value == "1:2" { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info text-024.suz:2: 1
info text-024.suz:2: 2
```

### text-009 桁数の大きな整数と負数をas%でラップする

主根拠：`[symbol.cast-wrap]`。

**ソース：**

```suz
fn main() -> u8 {
    fix large = "18446744073709551617" as% u8 or 7
    fix negative = "-1" as% u8 or 7
    fix signed = "255" as% i8 or 7
    fix invalid = "18446744073709551617x" as% u8 or 7
    fix fraction = "1.0" as% u8 or 7
    if large == 1 && negative == 255 && signed == -1 && invalid == 7 && fraction == 7 { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-011 boolへのas%を拒否する

主根拠：`[symbol.cast-wrap]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = "true" as% bool or false
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
| 通常 | `[symbol.cast-wrap]` | 2 | 24 |

### text-067 symbol と text から整数型への飽和変換

主根拠：`[symbol.cast-sat]`。

補助根拠：`[text.cast-view]`、`[symbol.cast-handler]`。

**ソース：**

```suz
fn main() -> u8 {
    fix hi: u8 = "999" as| u8 or 1
    fix lo: u8 = "-5" as| u8 or 1
    fix bad: u8 = "-5x" as| u8 or 7
    fix t: text = "-200"
    fix s: i8 = t as| i8 or 0
    if hi != 255 || lo != 0 || bad != 7 || s != -128 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-008 成功する定数文字列のasにもorを要求する

主根拠：`[symbol.cast-handler]`。

**ソース：**

```suz
fix VALUE: u8 = "1" as u8
fn main() -> u8 { return VALUE }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[symbol.cast-handler]` | 1 | 21 |

### text-010 成功する定数文字列のas%にもorを要求する

主根拠：`[symbol.cast-handler]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = "256" as% u8
    return value
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[symbol.cast-handler]` | 2 | 23 |

### text-068 symbol からの飽和変換で or を省略すると拒否する

主根拠：`[symbol.cast-handler]`。

補助根拠：`[symbol.cast-sat]`。

**ソース：**

```suz
fn main() -> u8 {
    fix a: u8 = "10" as| u8
    return a
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[symbol.cast-handler]` | 2 | 22 |

### text-012 symbol変換の評価回数と代替値の遅延評価

主根拠：`[symbol.cast-handler]`。

**ソース：**

```suz
fn source(value: symbol) -> symbol {
    log info "source"
    return value
}
fn fallback() -> u8 {
    log info "fallback"
    return 7
}
fn main() -> u8 {
    fix first = source("1") as u8 or fallback()
    fix second = source("bad") as% u8 or fallback()
    if first == 1 && second == 7 { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info text-012.suz:2: source
info text-012.suz:2: source
info text-012.suz:6: fallback
```

### text-013 symbol変換のor returnで呼び出し元に戻る

主根拠：`[symbol.cast-handler]`。

**ソース：**

```suz
fn parse(value: symbol) -> u8 {
    fix result = value as u8 or return 7
    return result
}
fn main() -> u8 {
    if parse("2") == 2 && parse("bad") == 7 { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-005 数値とboolをSuzの表記で読み取る

主根拠：`[symbol.cast-format]`。

**ソース：**

```suz
fn main() -> u8 {
    fix integer = "1_000" as u16 or 0
    fix minimum = "-128" as i8 or 0
    fix fraction = "1.2_5e+1" as f64 or 0.0
    fix integral = "1" as f32 or 0.0
    fix yes = "true" as bool or false
    fix no = "false" as bool or true
    if integer == 1000 && minimum == -128 && fraction == 12.5 && integral == 1.0 && yes && not no { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-006 数値とboolの入力全体を検査する

主根拠：`[symbol.cast-format]`。

**ソース：**

```suz
fn invalid(value: symbol) -> bool {
    return (value as u8 or 99) == 99
}
fn main() -> u8 {
    if not (invalid("") && invalid(" 1") && invalid("1 ")) { return 1 }
    if not (invalid("1\0") && invalid("1+2") && invalid("(1)")) { return 2 }
    if not (invalid("+1") && invalid("1__0") && invalid("1.0")) { return 3 }
    if not (invalid("0x10") && invalid("1//comment")) { return 4 }
    if ("True" as bool or false) { return 5 }
    if ("1" as bool or false) { return 6 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-007 整数の上下限と浮動小数点の丸めを確認する

主根拠：`[symbol.cast-range]`。

**ソース：**

```suz
fn main() -> u8 {
    fix maximum = "18446744073709551615" as u64 or 0
    fix overflow = "18446744073709551616" as u64 or 7
    fix underflow = "-129" as i8 or 7
    fix unsigned = "-0" as u8 or 7
    fix rounded = "16777217" as f32 or 0.0
    fix too_large = "1.0e100" as f32 or 7.0
    fix tiny = "1.0e-1000" as f64 or 7.0
    fix negative_zero = "-0" as f64 or 7.0
    fix special = "nan" as f64 or 0.0
    fix infinity = "-inf" as f64 or 0.0
    if maximum != 18446744073709551615 || overflow != 7 || underflow != 7 || unsigned != 7 { return 1 }
    if rounded != 16777216.0 || too_large != 7.0 || tiny != 0.0 { return 2 }
    if 1.0 / negative_zero != -inf || special == special || infinity != -inf { return 3 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-015 構造体へ入れ子の値とsymbolと列挙子を読み取る

主根拠：`[symbol.cast-struct]`。

**ソース：**

```suz
enum Mode: u8 { Fast, Slow }
struct Point {
    x: i32,
    y: i32,
}
struct Item {
    name: symbol,
    mode: Mode,
    point: Point,
}
fn main() -> u8 {
    fix input = "Item { point: Point { y: 2, x: 1 }, mode: Mode.Fast, name: \"Suz\" }"
    fix item = input as Item or return 1
    if item.name == "Suz" && item.mode == Mode.Fast && item.point.x == 1 && item.point.y == 2 { return 0 }
    return 2
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-016 構造体の型名とフィールドの過不足を検査する

主根拠：`[symbol.cast-struct]`。

**ソース：**

```suz
struct Point {
    x: u8,
    y: u8,
}
fn invalid(value: symbol) -> bool {
    fix point = value as Point or Point { x: 99, y: 99 }
    return point.x == 99 && point.y == 99
}
fn main() -> u8 {
    if not (invalid("Other { x: 1, y: 2 }") && invalid("Point { x: 1 }")) { return 1 }
    if not (invalid("Point { x: 1, x: 2, y: 3 }") && invalid("Point { x: 1, y: 2, z: 3 }")) { return 2 }
    if not (invalid("Point { x: 256, y: 2 }") && invalid("Point { x: 1+2, y: 3 }")) { return 3 }
    if not (invalid("Point { x: 1, y: 2 }tail") && invalid("Point { x: 1, y: wrong }")) { return 4 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-017 構造体内の文字列でエスケープを展開する

主根拠：`[symbol.cast-struct-symbol]`。

**ソース：**

```suz
struct Label {
    name: symbol,
}
fn main() -> u8 {
    fix input = "Label { name: \"a\\0b\" }"
    fix label = input as Label or return 1
    if label.name == "a\0b" && label.name != "a" { return 0 }
    return 2
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-018 変換入力内の不正エスケープと埋め込みを失敗にする

主根拠：`[symbol.cast-struct-symbol]`。

ソースのエスケープは合法である。復元された変換入力の不正をorが処理する。

**ソース：**

```suz
struct Label {
    name: symbol,
}
fn invalid(value: symbol) -> bool {
    fix label = value as Label or Label { name: "fallback" }
    return label.name == "fallback"
}
fn main() -> u8 {
    if not invalid("Label { name: \"\\q\" }") { return 1 }
    if not invalid("Label { name: \"\${1}\" }") { return 2 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-019 構造体入力の行数に応じて末尾カンマを検査する

主根拠：`[symbol.cast-struct-layout]`。

**ソース：**

```suz
struct Point {
    x: u8,
    y: u8,
}
fn valid(value: symbol) -> bool {
    fix point = value as Point or return false
    return point.x == 1 && point.y == 2
}
fn main() -> u8 {
    if not valid("Point { x: 1, y: 2 }") { return 1 }
    if not valid("Point {\nx: 1,\ny: 2,\n}") { return 2 }
    if not valid("Point {\r\nx: 1,\r\ny: 2,\r\n}") { return 3 }
    if valid("Point { x: 1, y: 2, }") { return 4 }
    if valid("Point {\nx: 1,\ny: 2\n}") { return 5 }
    if valid("Point { x: 1\n, y: 2,\n}") { return 6 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-027 Unicodeの部分範囲を切り出して順逆に読む

主根拠：`[text.slice]`。

**ソース：**

```suz
fn tail(value: text) -> text { return value.[1:] or "" }
fn main() -> u8 {
    fix source = "xあ😀\0z"
    fix body = tail(source)
    if (body.[:2] or "") != "あ😀" { return 1 }
    if (body.[2] or '?') != '\0' { return 2 }
    if ((body.[1:] or "").[0] or '?') != '😀' { return 3 }
    var count: u8 = 0
    for ch in body { count += 1 }
    rev ch in body {
        count += 1
        continue
    }
    if count != 8 { return 4 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-031 対象と両端を順に評価し、代替値を必要時だけ使う

主根拠：`[text.slice]`。

**ソース：**

```suz
fn source() -> symbol {
    log info "source"
    return "abc"
}
fn bound(value: i64) -> i64 {
    log info "${value}"
    return value
}
fn fallback() -> text {
    log info "fallback"
    return ""
}
fn main() -> u8 {
    fix first = source().[bound(0):bound(1)] or fallback()
    fix second = source().[bound(2):bound(1)] or fallback()
    if first != "a" { return 1 }
    if second != "" { return 2 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info text-031.suz:2: source
info text-031.suz:6: 0
info text-031.suz:6: 1
info text-031.suz:2: source
info text-031.suz:6: 2
info text-031.suz:6: 1
info text-031.suz:10: fallback
```
### text-028 空の範囲と不正な位置を処理する

主根拠：`[text.slice-fail]`。

**ソース：**

```suz
fn main() -> u8 {
    fix source = "abc"
    if (source.[3:] or "bad") != "" { return 1 }
    if (source.[-1:] or "bad") != "bad" { return 2 }
    if (source.[2:1] or "bad") != "bad" { return 3 }
    if (source.[:4] or "bad") != "bad" { return 4 }
    if (source.[3] or '?') != '?' { return 5 }
    fix empty = "".[:]
    if (empty.[0] or '?') != '?' { return 6 }
    fix result = source.[4:] or return 0
    return 7
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-032 切り出しには成功する位置でもorを要求する

主根拠：`[text.slice-handler]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = "abc".[:1]
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
| 通常 | `[text.slice-handler]` | 2 | 22 |

### text-029 切り出した範囲を数値・真偽値・文字・構造体として読む

主根拠：`[text.cast-view]`。

**ソース：**

```suz
struct Item { name: text }
fn number(value: text) -> u8 { return value as u8 or 99 }
fn item(value: text) -> Item { return value as Item or Item { name: "bad" } }
fn main() -> u8 {
    fix source = "x42y"
    if number(source.[1:3] or "") != 42 { return 1 }
    if (("xtruey".[1:5] or "") as bool or false) != true { return 2 }
    if (("x😀y".[1:2] or "") as char or '?') != '😀' { return 3 }
    if (("x256y".[1:4] or "") as% u8 or 7) != 0 { return 4 }
    fix parsed = item("Item { name: \"Suz\" }".[:])
    if parsed.name != "Suz" { return 5 }
    if (parsed.name as symbol) != "Suz" { return 6 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-030 定数の切り出しとsymbol・textのパターン照合

主根拠：`[text.const-view]`。

**ソース：**

```suz
fix SOURCE = "xnamey"
fix KEY = SOURCE.[1:5] or ""
fix FIRST = KEY.[0] or '?'
fix FALLBACK = SOURCE.[-1:] or "bad"
fix INTERNED = KEY as symbol
fn classify(value: symbol) -> u8 {
    return matchexpr value {
        KEY => 1
        _ => 0
    }
}
fn main() -> u8 {
    if FIRST != 'n' { return 1 }
    if FALLBACK != "bad" { return 2 }
    if INTERNED != "name" { return 3 }
    if classify("name") != 1 { return 4 }
    fix view = SOURCE.[1:5] or return 5
    return matchexpr view {
        "name" => 0
        _ => 6
    }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-022 内容が等しいsymbolパターンの重複を拒否する

主根拠：`[match.pattern]`。

**ソース：**

```suz
fix NAME: symbol = "ready"
fn main() -> u8 {
    match NAME {
        NAME => { return 0 }
        "${NAME}" => { return 1 }
        _ => { return 2 }
    }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[match.pattern]` | 5 | 9 |

### text-023 symbolのmatchに最後のワイルドカードを要求する

主根拠：`[match.exhaustive]`。

**ソース：**

```suz
fix STATE: symbol = "ready"
fn main() -> u8 {
    match STATE {
        "ready" => { return 0 }
    }
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
| 通常 | `[match.exhaustive]` | 3 | 5 |

### text-021 symbolをmatchとmatchexprで分岐する

主根拠：`[match.target]`。

**ソース：**

```suz
fn classify(value: symbol) -> u8 {
    return matchexpr value {
        "ready" => 1
        "busy" => 2
        _ => 3
    }
}
fn main() -> u8 {
    var result: u8 = 0
    match "busy" {
        "ready" => { result = 1 }
        "busy" => { result = 2 }
        _ => { result = 3 }
    }
    if result == 2 && classify("ready") == 1 && classify("other") == 3 { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-025 symbolをログへ埋め込む

主根拠：`[log.fragments]`。

**ソース：**

```suz
fn main() -> u8 {
    fix message = "Hello"
    log info "${message}"
    fix count = 1
    log info "count: ${count}"
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info text-025.suz:3: Hello
info text-025.suz:5: count: 1
```

### text-034 lenはUnicodeスカラー値を数え定数式にも使える

主根拠：`[len.expr]`。

対象型・定数評価・優先順位は `[len.targets]`、`[len.const]`、`[len.precedence]` も確認する。

バイト数や見た目の文字数ではなく、Unicodeスカラー値を数える。関数呼び出しは一度だけ評価する。

**ソース：**

```suz
fix COUNT = len "あ😀x"
fix TAIL = len "あ😀x".[1:] or ""
fn message() -> symbol {
    log info "called"
    return "あ😀x"
}
fn main() -> u8 {
    fix n = len message()
    fix width: i32 = 80 - n
    fix half: f64 = n * 0.5
    if COUNT != 3 || TAIL != 2 || n != 3 { return 1 }
    if width != 77 || half != 1.5 { return 2 }
    if len "" != 0 || len "é" != 2 { return 3 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info text-034.suz:4: called
```

### text-035 切り出しの開始と終了は異なる整数型を使える

主根拠：`[text.slice-bounds]`。

各位置を独立に評価する。負の値や大きな符号なし整数は範囲外として処理する。

**ソース：**

```suz
fn main() -> u8 {
    fix value = "あ😀x"
    fix start: u8 = 1
    fix end: len_t = len value
    fix huge: u64 = 18446744073709551615
    if (value.[start:end] or "") != "😀x" { return 1 }
    fix negative: i64 = -1
    if (value.[negative:end] or "bad") != "bad" { return 2 }
    if (value.[start:huge] or "bad") != "bad" { return 3 }
    if (value.[end] or '?') != '?' { return 4 }
    fix tiny_end: i8 = 2
    if (value.[300:tiny_end] or "bad") != "bad" { return 5 }
    if (value.[tiny_end:300] or "bad") != "bad" { return 6 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-036 len_tの範囲を検査し汎用関数と反復に使える

主根拠：`[len.type-operations]`。

汎用関数と変換は `[len.type-generic]`、`[len.type-cast]` も確認する。

**ソース：**

```suz
fn identity(value: int) -> int { return value }
fn parse(value: text) -> len_t { return value as len_t or 7 }
fn main() -> u8 {
    fix hi: len_t = 2147483647
    fix zero: len_t = 0
    fix over: len_t = hi + 1 or 9
    fix under: len_t = zero - 1 or 9
    if over != 9 || under != 9 { return 1 }
    if parse("2147483647") != hi || parse("2147483648") != 7 { return 2 }
    var count: len_t = 0
    for i in 0..<len "abc" { count += identity(i) }
    if count != 3 { return 3 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-037 関数から返るtextの利用期間を内側の作業領域から延ばさない

主根拠：`[arena.return-lifetime]`。

**ソース：**

```suz
fn identity(t: text) -> text { return t }
fn invalid(t: text) -> text {
    arena { return identity(t) }
}
fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[arena.return-lifetime]` | 3 | 20 |

### text-038 c_bytesへ変えても戻り値の利用期間は延びない

主根拠：`[arena.return-lifetime]`。

**ソース：**

```suz
fn bytes(t: text) -> *const void {
    trusted { return c_bytes t }
}
fn invalid(t: text) -> *const void {
    arena { return bytes(t) }
}
fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[arena.return-lifetime]` | 5 | 20 |

### text-039 静的な文字列の参照は内側の関数呼び出しで短命にならない

主根拠：`[arena.return-lifetime]`。

**ソース：**

```suz
struct Message { body: text }
fn identity(t: text) -> text { return t }
fn bytes(t: text) -> *const void {
    trusted { return c_bytes t }
}
fn main() -> u8 {
    var saved = Message { body: "" }
    trusted {
        var p = c_bytes ""
        arena {
            saved#body = identity("static")
            p = bytes(saved.body)
        }
    }
    if saved.body == "static" { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-040 symbolを要求する場所だけで実行時テンプレートをインターン化する

主根拠：`[symbol.template]`。

**ソース：**

```suz
struct Label { name: symbol }
fn echo(s: symbol) -> symbol { return s }
fn make(n: u8) -> symbol { return "${n}" }
fn main() -> u8 {
    fix n: u8 = 3
    var s: symbol = "${n}"
    s = "${n}x"
    fix item = Label { name: "${n}" }
    fix changed = item with { name: "${n}x" }
    fix items: #[symbol] = new #["${n}", "${n}x"]
    fix choice: symbol = ifexpr true { "${n}" } else { "${n}x" }
    fix selected: symbol = matchexpr true { true => "${n}"
false => "${n}x" }
    if s != "3x" || changed.name != s || items.[1] != s { return 1 }
    if choice != echo("${n}") || selected != make(n) { return 2 }
    fix joined = ifexpr true { "static" } else { s }
    if joined != "static" { return 3 }
    match choice { "3" => {}
_ => { return 4 } }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-041 型指定なしの実行時テンプレートにはtextbufが必要

主根拠：`[symbol.template]`。

**ソース：**

```suz
fn main() -> u8 {
    fix n: u8 = 1
    fix s: symbol = "name"
    fix t = "${n}"
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
| 通常 | `[symbol.template]` | 4 | 13 |

### text-042 asの変換先はテンプレートの要求型にならない

主根拠：`[symbol.template]`。

**ソース：**

```suz
fn main() -> u8 {
    fix n: u8 = 1
    fix s: symbol = "name"
    fix t = "${n}" as symbol
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
| 通常 | `[symbol.template]` | 4 | 13 |

### text-043 他方の分岐のsymbolはテンプレートの要求型にならない

主根拠：`[symbol.template]`。

**ソース：**

```suz
fn main() -> u8 {
    fix n: u8 = 1
    fix s: symbol = "name"
    fix t = ifexpr true { "${n}" } else { s }
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
| 通常 | `[symbol.template]` | 4 | 27 |

### text-044 比較はテンプレートへsymbolを要求しない

主根拠：`[symbol.template]`。

**ソース：**

```suz
fn main() -> u8 {
    fix n: u8 = 1
    fix s: symbol = "name"
    fix t = s == "${n}"
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
| 通常 | `[symbol.template]` | 4 | 18 |

### text-045 textbuf内の入れ子テンプレートにも生成先が必要

主根拠：`[symbol.template]`。

**ソース：**

```suz
fn main() -> u8 {
    fix n: u8 = 1
    fix s: symbol = "name"
    fix t = textbuf "${"${n}"}"
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
| 通常 | `[symbol.template]` | 4 | 24 |

### text-046 textbufは定数式ではない

主根拠：`[const.expression]`。

**ソース：**

```suz
fix T = textbuf "constant"
fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[const.expression]` | 1 | 9 |

### text-047 作業領域のtextを外側へ保存しない

主根拠：`[arena.store]`。

**ソース：**

```suz
fn main() -> u8 {
    var saved: text = ""
    arena { saved = textbuf "inner" }
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
| 通常 | `[arena.store]` | 3 | 13 |

### text-048 切り出してもtextの寿命は延びない

主根拠：`[arena.store]`。

**ソース：**

```suz
fn main() -> u8 {
    var saved: text = ""
    arena { saved = (textbuf "inner").[1:] or "" }
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
| 通常 | `[arena.store]` | 3 | 13 |

### text-049 関数で作ったtextを外側へ保存しない

主根拠：`[arena.store]`。

**ソース：**

```suz
fn copy(t: text) -> text { return textbuf t }
fn main() -> u8 {
    var saved: text = ""
    arena { saved = copy("inner") }
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
| 通常 | `[arena.store]` | 4 | 13 |

### text-050 構造体変換で作ったtextを外側へ保存しない

主根拠：`[arena.store]`。

**ソース：**

```suz
struct Item { body: text }
fn parse(t: text) -> Item { return t as Item or Item { body: "" } }
fn main() -> u8 {
    var saved: text = ""
    fix input = "Item { body: \"inner\" }"
    arena { saved = parse(input).body }
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
| 通常 | `[arena.store]` | 6 | 13 |

### text-051 c_bytesもtextの実体の寿命に従う

主根拠：`[arena.store]`。

**ソース：**

```suz
fn main() -> u8 {
    trusted {
        var saved = c_bytes ""
        arena { saved = c_bytes textbuf "inner" }
    }
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
| 通常 | `[arena.store]` | 4 | 17 |

### text-052 反復ごとに回収しないtextbufの確保を警告する

主根拠：`[arena.loop-allocation]`。

**ソース：**

```suz
fn main() -> u8 {
    for i in 0..<2 { fix t = textbuf "${i}" }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 1 |

**警告：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[arena.loop-allocation]` | 2 | 30 |

### text-053 追記しても取得済みの別名と切り出しの内容を変えない

主根拠：`[text.append-storage]`。

**ソース：**

```suz
fn main() -> u8 {
    var a = textbuf[32] "あb"
    var b = a
    fix old = a
    b <- " B"
    a <- " A"
    if a != "あb A" || b != "あb B" || old != "あb" { return 1 }
    var suffix = b.[1:] or b
    suffix <- suffix
    if suffix != "b Bb B" || b != "あb B" { return 2 }
    var empty = textbuf ""
    empty <- "x"
    empty <- empty
    empty <- ""
    if empty != "xx" { return 3 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-054 関数を通る現在の作業領域の前提を実引数で満たす

主根拠：`[text.append-arena]`。

**ソース：**

```suz
struct Message { body: text }
fn append(t: text) -> text {
    var result = t
    result <- "!"
    return result
}
fn forward(t: text) -> text { return append(t) }
fnshape Transform(text) -> text
fn apply(f: fn Transform, t: text) -> text { return f(t) }
fn repeat(t: text, n: i32) -> text {
    if n == 0 { return t }
    return repeat(append(t), n - 1)
}
fn change(p: mut Message) -> void { p#body <- "?" }
fn main() -> u8 {
    fix t = textbuf "Hello"
    fix p = new Message { body: forward(t) }
    change(p)
    if apply(fn forward, t) != "Hello!" || repeat(t, 2) != "Hello!!" { return 3 }
    if p.body != "Hello!?" || t != "Hello" { return 1 }
    arena {
        var copied = textbuf t
        copied <- " inner"
        if copied != "Hello inner" { return 2 }
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-055 追記先を一度評価し旧値を保存してから埋め込みを評価する

主根拠：`[text.append]`。

**ソース：**

```suz
struct Message { body: text, n: i32 }
fn target(p: mut Message) -> mut Message {
    p#n += 1
    return p
}
fn part(p: mut Message) -> i32 {
    p#n += 1
    p#body <- "x"
    return p.n
}
fn main() -> u8 {
    fix p = new Message { body: textbuf[32] "A", n: 0 }
    target(p)#body <- ("${part(p)}:${part(p)}")
    if p.n != 3 || p.body != "A2:3" { return 1 }
    var item = Message { body: textbuf "B", n: 0 }
    item#body <- "C"
    fix value = new textbuf "D"
    #value <- "E"
    if item.body != "BC" || .value != "DE" { return 2 }
    fix texts = new #[textbuf "F"]
    texts#[0] <- "G"
    if texts.[0] != "FG" { return 3 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-056 静的なtextには空文字列も追記できない

主根拠：`[text.append-arena]`。

**ソース：**

```suz
fn main() -> u8 {
    var t = "static"
    t <- ""
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
| 通常 | `[text.append-arena]` | 3 | 7 |

### text-057 外側の実体は内側で宣言した変数にコピーしても追記できない

主根拠：`[text.append-arena]`。

**ソース：**

```suz
fn main() -> u8 {
    fix t = textbuf "outer"
    arena {
        var local = t
        local <- "x"
    }
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
| 通常 | `[text.append-arena]` | 5 | 15 |

### text-058 追記する関数へ静的なtextを渡せない

主根拠：`[text.append-arena]`。

**ソース：**

```suz
fn append(t: text) -> text {
    var result = t
    result <- "!"
    return result
}
fn forward(t: text) -> text { return append(t) }
fn main() -> u8 {
    fix result = forward("static")
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
| 通常 | `[text.append-arena]` | 8 | 18 |

### text-059 symbolから借りたtextには追記できない

主根拠：`[text.append-arena]`。

**ソース：**

```suz
fn main() -> u8 {
    fix s: symbol = "symbol"
    var t: text = s
    t <- "x"
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
| 通常 | `[text.append-arena]` | 4 | 7 |

### text-060 分岐の一方が静的textなら現在の作業領域にあると判定しない

主根拠：`[text.append-arena]`。

**ソース：**

```suz
fn choose(t: text, yes: bool) -> text {
    if yes { return t }
    return "static"
}
fn main() -> u8 {
    var t = choose(textbuf "arena", true)
    t <- "x"
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
| 通常 | `[text.append-arena]` | 7 | 7 |

### text-061 symbolを追記先にできない

主根拠：`[text.append]`。

**ソース：**

```suz
fn main() -> u8 {
    var s: symbol = "symbol"
    s <- "x"
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
| 通常 | `[text.append]` | 3 | 7 |

### text-062 追記の左辺は書き込み可能でなければならない

主根拠：`[var.fix-assign]`。

**ソース：**

```suz
fn main() -> u8 {
    fix t = textbuf "fixed"
    t <- "x"
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
| 通常 | `[var.fix-assign]` | 3 | 5 |

### text-063 追記の右辺の失敗時は更新せず関数から戻る

主根拠：`[text.append]`。

**ソース：**

```suz
struct Message { body: text }
fn update(p: mut Message, t: text, end: i32) -> i32 {
    p#body <- t.[:end] or return 7
    return 0
}
fn main() -> u8 {
    fix p = new Message { body: textbuf "A" }
    if update(p, "BC", 9) != 7 || p.body != "A" { return 1 }
    if update(p, "BC", 1) != 0 || p.body != "AB" { return 2 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-064 反復の外へ残らない追記による確保を警告する

主根拠：`[arena.loop-allocation]`。

**ソース：**

```suz
fn main() -> u8 {
    fix base = textbuf "A"
    for i in 0..<2 {
        var t = base
        t <- "${i}"
    }
    for i in 0..<2 {
        arena {
            var t = textbuf "A"
            t <- "${i}"
        }
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 1 |

**警告：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[arena.loop-allocation]` | 5 | 11 |

### text-065 内側の作業領域で呼ぶ関数へ外側のtextを渡して追記できない

主根拠：`[text.append-arena]`。

**ソース：**

```suz
struct Message { body: text }
fn append(p: mut Message) -> void { p#body <- "!" }
fn main() -> u8 {
    fix p = new Message { body: textbuf "outer" }
    arena { append(p) }
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
| 通常 | `[text.append-arena]` | 5 | 13 |

### text-066 空白のないx<-1は比較ではなく追記として読む

主根拠：`[text.append]`。

補助根拠：`[src.longest-match]`。

**ソース：**

```suz
fn main() -> u8 {
    fix x: i32 = 0
    if x<-1 { return 1 }
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
| 通常 | `[text.append]` | 3 | 9 |
