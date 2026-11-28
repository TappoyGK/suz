# 仕様例：symbol と text（基本操作・変換・切り出し）

### text-001 関数から返したsymbolを構造体へ保存してコピーする
- 主根拠：`[symbol.copy]`
- 結果：正常終了

関数の復帰、元変数への再代入、構造体フィールドの更新後もコピー先の内容が変わらない。

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

### text-002 異なる生成経路の同値と空文字列を比較する
- 主根拠：`[symbol.compare]`
- 結果：正常終了

これは比較結果の確認である。実体の共有そのものはランタイムを直接検証するテストで確認する。

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

### text-003 Unicodeの辞書順と接頭辞を比較する
- 主根拠：`[symbol.order]`
- 結果：正常終了

「é」はU+00E9、分解形はU+0065とU+0301。「𐀀」はU+10000、「」はU+E000である。UTF-16のコード単位順との取り違えとUnicode正規化を検出する。

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

### text-004 symbolを加算演算で連結しようとする
- 主根拠：`[symbol.operations]`
- 結果：コンパイルエラー `[symbol.operations]` 4:22

```suz
fn main() -> u8 {
    fix left: symbol = "a"
    fix right: symbol = "b"
    fix value = left + right
    return 0
}
```

### text-026 symbolをtextとして渡し、分岐と構造体に保持する
- 主根拠：`[text.from-symbol]`
- 結果：正常終了

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

### text-033 textをsymbolへ暗黙変換しない
- 主根拠：`[text.from-symbol]`
- 結果：コンパイルエラー `[type.expected]` 2:30

```suz
fn main() -> u8 {
    fix value: symbol = "abc".[:]
    return 0
}
```

### text-020 constでテンプレートと型変換を評価する
- 主根拠：`[const.symbol-cast]`
- 結果：正常終了

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

### text-014 テンプレートの埋め込みにor returnを置く
- 主根拠：`[expr.or-return-position]`
- 結果：コンパイルエラー `[expr.or-return-position]` 2:43

```suz
fn main() -> u8 {
    fix value: symbol = "${"bad" as u8 or return 1}"
    return 0
}
```

### text-009 桁数の大きな整数と負数をas%でラップする
- 主根拠：`[symbol.cast-wrap]`
- 結果：正常終了

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

### text-011 boolへのas%を拒否する
- 主根拠：`[symbol.cast-wrap]`
- 結果：コンパイルエラー `[symbol.cast-wrap]` 2:24

```suz
fn main() -> u8 {
    fix value = "true" as% bool or false
    return 0
}
```

### text-067 symbol と text から整数型への飽和変換
- 主根拠：`[symbol.cast-sat]`
- 補助根拠：`[text.cast-view]`、`[symbol.cast-handler]`
- 結果：正常終了

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

### text-008 成功する定数文字列のasにもorを要求する
- 主根拠：`[symbol.cast-handler]`
- 結果：コンパイルエラー `[symbol.cast-handler]` 1:21

```suz
fix VALUE: u8 = "1" as u8
fn main() -> u8 { return VALUE }
```

### text-010 成功する定数文字列のas%にもorを要求する
- 主根拠：`[symbol.cast-handler]`
- 結果：コンパイルエラー `[symbol.cast-handler]` 2:23

```suz
fn main() -> u8 {
    fix value = "256" as% u8
    return value
}
```

### text-068 symbol からの飽和変換で or を省略すると拒否する
- 主根拠：`[symbol.cast-handler]`
- 補助根拠：`[symbol.cast-sat]`
- 結果：コンパイルエラー `[symbol.cast-handler]` 2:22

```suz
fn main() -> u8 {
    fix a: u8 = "10" as| u8
    return a
}
```

### text-012 symbol変換の評価回数と代替値の遅延評価
- 主根拠：`[symbol.cast-handler]`
- 結果：正常終了

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

**標準エラー：**

```text
info text-012.suz:2: source
info text-012.suz:2: source
info text-012.suz:6: fallback
```

### text-013 symbol変換のor returnで呼び出し元に戻る
- 主根拠：`[symbol.cast-handler]`
- 結果：正常終了

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

### text-005 数値とboolをSuzの表記で読み取る
- 主根拠：`[symbol.cast-format]`
- 結果：正常終了

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

### text-006 数値とboolの入力全体を検査する
- 主根拠：`[symbol.cast-format]`
- 結果：正常終了

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

### text-007 整数の上下限と浮動小数点の丸めを確認する
- 主根拠：`[symbol.cast-range]`
- 結果：正常終了

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

### text-015 構造体へ入れ子の値とsymbolと列挙子を読み取る
- 主根拠：`[symbol.cast-struct]`
- 結果：正常終了

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

### text-016 構造体の型名とフィールドの過不足を検査する
- 主根拠：`[symbol.cast-struct]`
- 結果：正常終了

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

### text-017 構造体内の文字列でエスケープを展開する
- 主根拠：`[symbol.cast-struct-symbol]`
- 結果：正常終了

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

### text-018 変換入力内の不正エスケープと埋め込みを失敗にする
- 主根拠：`[symbol.cast-struct-symbol]`
- 結果：正常終了

ソースのエスケープは合法である。復元された変換入力の不正をorが処理する。

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

### text-019 構造体入力の行数に応じて末尾カンマを検査する
- 主根拠：`[symbol.cast-struct-layout]`
- 結果：正常終了

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

### text-027 Unicodeの部分範囲を切り出して順逆に読む
- 主根拠：`[text.slice]`
- 結果：正常終了

```suz
fn tail(value: text) -> text { return value.[1:] or "" }
fn main() -> u8 {
    fix source = "xあ😀\0z"
    fix body = tail(source)
    if (body.[:2] or "") != "あ😀" { return 1 }
    if (body.[2] or \?) != \nul { return 2 }
    if ((body.[1:] or "").[0] or \?) != \😀 { return 3 }
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

### text-031 対象と両端を順に評価し、代替値を必要時だけ使う
- 主根拠：`[text.slice]`
- 結果：正常終了

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
- 主根拠：`[text.slice-fail]`
- 結果：正常終了

```suz
fn main() -> u8 {
    fix source = "abc"
    if (source.[3:] or "bad") != "" { return 1 }
    if (source.[-1:] or "bad") != "bad" { return 2 }
    if (source.[2:1] or "bad") != "bad" { return 3 }
    if (source.[:4] or "bad") != "bad" { return 4 }
    if (source.[3] or \?) != \? { return 5 }
    fix empty = "".[:]
    if (empty.[0] or \?) != \? { return 6 }
    fix result = source.[4:] or return 0
    return 7
}
```

### text-032 切り出しには成功する位置でもorを要求する
- 主根拠：`[text.slice-handler]`
- 結果：コンパイルエラー `[text.slice-handler]` 2:22

```suz
fn main() -> u8 {
    fix value = "abc".[:1]
    return 0
}
```

### text-029 切り出した範囲を数値・真偽値・文字・構造体として読む
- 主根拠：`[text.cast-view]`
- 結果：正常終了

```suz
struct Item { name: text }
fn number(value: text) -> u8 { return value as u8 or 99 }
fn item(value: text) -> Item { return value as Item or Item { name: "bad" } }
fn main() -> u8 {
    fix source = "x42y"
    if number(source.[1:3] or "") != 42 { return 1 }
    if (("xtruey".[1:5] or "") as bool or false) != true { return 2 }
    if (("x😀y".[1:2] or "") as char or \?) != \😀 { return 3 }
    if (("x256y".[1:4] or "") as% u8 or 7) != 0 { return 4 }
    fix parsed = item("Item { name: \"Suz\" }".[:])
    if parsed.name != "Suz" { return 5 }
    if (parsed.name as symbol) != "Suz" { return 6 }
    return 0
}
```

### text-030 定数の切り出しとsymbol・textのパターン照合
- 主根拠：`[text.const-view]`
- 結果：正常終了

```suz
fix SOURCE = "xnamey"
fix KEY = SOURCE.[1:5] or ""
fix FIRST = KEY.[0] or \?
fix FALLBACK = SOURCE.[-1:] or "bad"
fix INTERNED = KEY as symbol
fn classify(value: symbol) -> u8 {
    return matchexpr value {
        KEY => 1
        _ => 0
    }
}
fn main() -> u8 {
    if FIRST != \n { return 1 }
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

### text-022 内容が等しいsymbolパターンの重複を拒否する
- 主根拠：`[match.pattern]`
- 結果：コンパイルエラー `[match.pattern]` 5:9

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

### text-023 symbolのmatchに最後のワイルドカードを要求する
- 主根拠：`[match.exhaustive]`
- 結果：コンパイルエラー `[match.exhaustive]` 3:5

```suz
fix STATE: symbol = "ready"
fn main() -> u8 {
    match STATE {
        "ready" => { return 0 }
    }
    return 1
}
```

### text-021 symbolをmatchとmatchexprで分岐する
- 主根拠：`[match.target]`
- 結果：正常終了

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

### text-034 lenはUnicodeスカラー値を数え定数式にも使える
- 主根拠：`[len.expr]`
- 結果：正常終了

対象型・定数評価・優先順位は `[len.targets]`、`[len.const]`、`[len.precedence]` も確認する。
バイト数や見た目の文字数ではなく、Unicodeスカラー値を数える。関数呼び出しは一度だけ評価する。

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

**標準エラー：**

```text
info text-034.suz:4: called
```

### text-035 切り出しの開始と終了は異なる整数型を使える
- 主根拠：`[text.slice-bounds]`
- 結果：正常終了

各位置を独立に評価する。負の値や大きな符号なし整数は範囲外として処理する。

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
    if (value.[end] or \?) != \? { return 4 }
    fix tiny_end: i8 = 2
    if (value.[300:tiny_end] or "bad") != "bad" { return 5 }
    if (value.[tiny_end:300] or "bad") != "bad" { return 6 }
    return 0
}
```

### text-036 len_tの範囲を検査し汎用関数と反復に使える
- 主根拠：`[len.type-operations]`
- 結果：正常終了

汎用関数と変換は `[len.type-generic]`、`[len.type-cast]` も確認する。

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

### text-046 textbufは定数式ではない
- 主根拠：`[const.expression]`
- 結果：コンパイルエラー `[const.expression]` 1:9

```suz
fix T = textbuf "constant"
fn main() -> u8 { return 0 }
```

### text-062 追記の左辺は書き込み可能でなければならない
- 主根拠：`[var.fix-assign]`
- 結果：コンパイルエラー `[var.fix-assign]` 3:5

```suz
fn main() -> u8 {
    fix t = textbuf "fixed"
    t <- "x"
    return 0
}
```
