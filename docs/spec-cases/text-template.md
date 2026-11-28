# symbolとtext（テンプレート・追記・textbuf・寿命）

[目次と書式](../spec-cases.md)

### text-024 symbol生成でも埋め込みを左から右へ1回ずつ評価する
- 主根拠：`[symbol.interpolation]`
- 結果：正常終了

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

**標準エラー：**

```text
info text-024.suz:2: 1
info text-024.suz:2: 2
```

### text-025 symbolをログへ埋め込む
- 主根拠：`[log.fragments]`
- 結果：正常終了

```suz
fn main() -> u8 {
    fix message = "Hello"
    log info "${message}"
    fix count = 1
    log info "count: ${count}"
    return 0
}
```

**標準エラー：**

```text
info text-025.suz:3: Hello
info text-025.suz:5: count: 1
```

### text-037 関数から返るtextの利用期間を内側の作業領域から延ばさない
- 主根拠：`[arena.return-lifetime]`
- 結果：コンパイルエラー `[arena.return-lifetime]` 3:20

```suz
fn identity(t: text) -> text { return t }
fn invalid(t: text) -> text {
    arena { return identity(t) }
}
fn main() -> u8 { return 0 }
```

### text-038 c_bytesへ変えても戻り値の利用期間は延びない
- 主根拠：`[arena.return-lifetime]`
- 結果：コンパイルエラー `[arena.return-lifetime]` 5:20

```suz
fn bytes(t: text) -> *const void {
    trusted { return c_bytes t }
}
fn invalid(t: text) -> *const void {
    arena { return bytes(t) }
}
fn main() -> u8 { return 0 }
```

### text-039 静的な文字列の参照は内側の関数呼び出しで短命にならない
- 主根拠：`[arena.return-lifetime]`
- 結果：正常終了

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

### text-040 symbolを要求する場所だけで実行時テンプレートをインターン化する
- 主根拠：`[symbol.template]`
- 結果：正常終了

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

### text-041 型指定なしの実行時テンプレートにはtextbufが必要
- 主根拠：`[symbol.template]`
- 結果：コンパイルエラー `[symbol.template]` 4:13

```suz
fn main() -> u8 {
    fix n: u8 = 1
    fix s: symbol = "name"
    fix t = "${n}"
    return 0
}
```

### text-042 asの変換先はテンプレートの要求型にならない
- 主根拠：`[symbol.template]`
- 結果：コンパイルエラー `[symbol.template]` 4:13

```suz
fn main() -> u8 {
    fix n: u8 = 1
    fix s: symbol = "name"
    fix t = "${n}" as symbol
    return 0
}
```

### text-043 他方の分岐のsymbolはテンプレートの要求型にならない
- 主根拠：`[symbol.template]`
- 結果：コンパイルエラー `[symbol.template]` 4:27

```suz
fn main() -> u8 {
    fix n: u8 = 1
    fix s: symbol = "name"
    fix t = ifexpr true { "${n}" } else { s }
    return 0
}
```

### text-044 比較はテンプレートへsymbolを要求しない
- 主根拠：`[symbol.template]`
- 結果：コンパイルエラー `[symbol.template]` 4:18

```suz
fn main() -> u8 {
    fix n: u8 = 1
    fix s: symbol = "name"
    fix t = s == "${n}"
    return 0
}
```

### text-045 textbuf内の入れ子テンプレートにも生成先が必要
- 主根拠：`[symbol.template]`
- 結果：コンパイルエラー `[symbol.template]` 4:24

```suz
fn main() -> u8 {
    fix n: u8 = 1
    fix s: symbol = "name"
    fix t = textbuf "${"${n}"}"
    return 0
}
```

### text-047 作業領域のtextを外側へ保存しない
- 主根拠：`[arena.store]`
- 結果：コンパイルエラー `[arena.store]` 3:13

```suz
fn main() -> u8 {
    var saved: text = ""
    arena { saved = textbuf "inner" }
    return 0
}
```

### text-048 切り出してもtextの寿命は延びない
- 主根拠：`[arena.store]`
- 結果：コンパイルエラー `[arena.store]` 3:13

```suz
fn main() -> u8 {
    var saved: text = ""
    arena { saved = (textbuf "inner").[1:] or "" }
    return 0
}
```

### text-049 関数で作ったtextを外側へ保存しない
- 主根拠：`[arena.store]`
- 結果：コンパイルエラー `[arena.store]` 4:13

```suz
fn copy(t: text) -> text { return textbuf t }
fn main() -> u8 {
    var saved: text = ""
    arena { saved = copy("inner") }
    return 0
}
```

### text-050 構造体変換で作ったtextを外側へ保存しない
- 主根拠：`[arena.store]`
- 結果：コンパイルエラー `[arena.store]` 6:13

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

### text-051 c_bytesもtextの実体の寿命に従う
- 主根拠：`[arena.store]`
- 結果：コンパイルエラー `[arena.store]` 4:17

```suz
fn main() -> u8 {
    trusted {
        var saved = c_bytes ""
        arena { saved = c_bytes textbuf "inner" }
    }
    return 0
}
```

### text-052 反復ごとに回収しないtextbufの確保を警告する
- 主根拠：`[arena.loop-allocation]`
- 結果：正常終了 / 警告：`[arena.loop-allocation]` 2:30

```suz
fn main() -> u8 {
    for i in 0..<2 { fix t = textbuf "${i}" }
    return 0
}
```

### text-053 追記しても取得済みの別名と切り出しの内容を変えない
- 主根拠：`[text.append-storage]`
- 結果：正常終了

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

### text-054 関数を通る現在の作業領域の前提を実引数で満たす
- 主根拠：`[text.append-arena]`
- 結果：正常終了

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

### text-055 追記先を一度評価し旧値を保存してから埋め込みを評価する
- 主根拠：`[text.append]`
- 結果：正常終了

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

### text-056 静的なtextには空文字列も追記できない
- 主根拠：`[text.append-arena]`
- 結果：コンパイルエラー `[text.append-arena]` 3:7

```suz
fn main() -> u8 {
    var t = "static"
    t <- ""
    return 0
}
```

### text-057 外側の実体は内側で宣言した変数にコピーしても追記できない
- 主根拠：`[text.append-arena]`
- 結果：コンパイルエラー `[text.append-arena]` 5:15

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

### text-058 追記する関数へ静的なtextを渡せない
- 主根拠：`[text.append-arena]`
- 結果：コンパイルエラー `[text.append-arena]` 8:18

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

### text-059 symbolから借りたtextには追記できない
- 主根拠：`[text.append-arena]`
- 結果：コンパイルエラー `[text.append-arena]` 4:7

```suz
fn main() -> u8 {
    fix s: symbol = "symbol"
    var t: text = s
    t <- "x"
    return 0
}
```

### text-060 分岐の一方が静的textなら現在の作業領域にあると判定しない
- 主根拠：`[text.append-arena]`
- 結果：コンパイルエラー `[text.append-arena]` 7:7

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

### text-061 symbolを追記先にできない
- 主根拠：`[text.append]`
- 結果：コンパイルエラー `[text.append]` 3:7

```suz
fn main() -> u8 {
    var s: symbol = "symbol"
    s <- "x"
    return 0
}
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

### text-063 追記の右辺の失敗時は更新せず関数から戻る
- 主根拠：`[text.append]`
- 結果：正常終了

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

### text-064 反復の外へ残らない追記による確保を警告する
- 主根拠：`[arena.loop-allocation]`
- 結果：正常終了 / 警告：`[arena.loop-allocation]` 5:11

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

### text-065 内側の作業領域で呼ぶ関数へ外側のtextを渡して追記できない
- 主根拠：`[text.append-arena]`
- 結果：コンパイルエラー `[text.append-arena]` 5:13

```suz
struct Message { body: text }
fn append(p: mut Message) -> void { p#body <- "!" }
fn main() -> u8 {
    fix p = new Message { body: textbuf "outer" }
    arena { append(p) }
    return 0
}
```

### text-066 空白のないx<-1は比較ではなく追記として読む
- 主根拠：`[text.append]`
- 補助根拠：`[src.longest-match]`
- 結果：コンパイルエラー `[text.append]` 3:9

```suz
fn main() -> u8 {
    fix x: i32 = 0
    if x<-1 { return 1 }
    return 0
}
```
