# メソッドと演算子メソッド

[目次と書式](../spec-cases.md)

### flow-090 メソッドの受け手とselfのキャプチャー

主根拠：`[method.eval-order]`。

**ソース：**

```suz
struct Counter { value: i32 }
struct Holder { counter: Counter, pointer: mut Counter }
enum Mode: u8 { Off, On }
fnshape Read() -> i32
fnshape Action() -> void
fn read(f: fn Read) -> i32 { return f() }
fn act(f: fn Action) -> void { f() }
fn Counter.get() -> i32 { return self.value }
fn Counter.get_after(ignored: i32) -> i32 { return self.value }
fn Counter#add(n: int) -> void { self#value += n as i32 or 0 }
fn Counter.echo(n: int) -> int { return n }
fn Counter.echo_float(n: float) -> float { return n }
fn Counter.sum(n: int) -> int {
    if n == 0 { return 0 }
    return self.sum(n - 1) + n
}
fn Counter#reset() -> void { self = Counter { value: 0 } }
fn Counter#twice() -> void { self#add(1 as i16)
    self#add(1 as i16) }
fn Counter#capture() -> i32 {
    act(fn [var self]() -> void { self#twice() })
    return read(fn [self]() -> i32 { return self.get() })
}
fn Counter#observe_alias(other: mut Counter) -> i32 {
    self#value = 30
    return other.value
}
fn Holder.bump_pointer() -> void { self.pointer#add(1 as u8) }
fn Mode.enabled() -> bool { return self == Mode.On }
fn Mode#toggle() -> void { self = ifexpr self == Mode.On { Mode.Off } else { Mode.On } }
fn main() -> u8 {
    var count = Counter { value: 1 }
    count#add(2 as i16)
    if count.get() != 3 || count.echo(9 as u16) != 9 { return 1 }
    if count.echo(1 as u8) + count.echo(300 as u16) != 301 { return 13 }
    if count.echo_float(1.25 as f32) + count.echo_float(2.5 as f64) != 3.75 { return 14 }
    if count.sum(3 as u8) != 6 { return 15 }
    if count#capture() != 5 || count.value != 5 { return 2 }
    count#reset()
    if count.value != 0 { return 3 }
    var mode = Mode.Off
    mode#toggle()
    if not mode.enabled() { return 4 }
    if not Mode.On.enabled() { return 5 }
    if (Counter { value: 8 }).get() != 8 { return 6 }
    arena {
        fix first = new Counter { value: 10 }
        fix second = new Counter { value: 20 }
        var selected = first
        selected#add(expr { selected = second
            2 as i16 })
        if first.value != 12 || second.value != 20 { return 7 }
        if first.get_after(expr { first#value = 15
            0 }) != 12 { return 8 }
        if first#observe_alias(first) != 30 { return 9 }
        var holder = Holder { counter: Counter { value: 2 }, pointer: first }
        holder.counter#add(3 as i8)
        holder.bump_pointer()
        if holder.counter.value != 5 || first.value != 31 { return 10 }
        fix maybe: mut? Counter = first
        let p = maybe { p#reset() }
        if first.value != 0 { return 11 }
        selected = expr { selected#reset()
            first }
        if selected.get() != 0 || second.get() != 0 { return 12 }
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-091 Arenaメソッドと失敗処理

主根拠：`[method.arena]`。

**ソース：**

```suz
struct Node { next: mut? Node, value: i32 }
error Trouble { Empty }
fn Node#append(value: i32) -> mut Node {
    fix child = new Node { next: nil, value }
    self#next = child
    return child
}
fn Node.follow() -> mut? Node { return self.next }
fn Node.read() -> i32 or Trouble {
    if self.value == 0 { fail Trouble.Empty }
    return self.value
}
fn make() -> mut Node { return new Node { next: nil, value: 0 } }
fn main() -> u8 {
    arena {
        fix p = make()
        fix child = p#append(9)
        if (child.read() or 0) != 9 { return 1 }
        fix next = p.follow()
        let n = next { if n.value != 9 { return 2 } } else { return 3 }
        if (p.read() or 7) != 7 { return 4 }
        make()#append(4)
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |
### method-001 読み取りメソッドと更新メソッド

主根拠：`[method.decl]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.length2() -> f32 { return self.x * self.x + self.y * self.y }
fn Vec2#add(other: Vec2) -> void { self#x += other.x
    self#y += other.y }
fn main() -> u8 {
    var p = Vec2 { x: 3.0, y: 4.0 }
    if p.length2() != 25.0 { return 1 }
    p#add(Vec2 { x: 1.0, y: 2.0 })
    if p.x != 4.0 || p.y != 6.0 { return 2 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### method-002 型名とメソッドの区切りを離さない

主根拠：`[method.decl]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2 .read() -> f32 { return self.x }
fn main() -> u8 {

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
| 通常 | `[method.decl]` | 2 | 10 |

### method-003 i32にはメソッドを定義できない

主根拠：`[method.target-type]`。

**ソース：**

```suz
fn i32.read() -> i32 { return 0 }
fn main() -> u8 {

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
| 通常 | `[method.target-type]` | 1 | 4 |

### method-004 Problemにはメソッドを定義できない

主根拠：`[method.target-type]`。

**ソース：**

```suz
error Problem { Bad }
fn Problem.read() -> i32 { return 0 }
fn main() -> u8 {

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
| 通常 | `[method.target-type]` | 2 | 4 |

### method-005 読み取りと更新で同じ名前を使わない

主根拠：`[method.name]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.length2() -> f32 { return self.x * self.x + self.y * self.y }
fn Vec2#length2() -> void {}
fn main() -> u8 {

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
| 通常 | `[method.name]` | 3 | 4 |

### method-006 フィールド名とメソッド名を重ねない

主根拠：`[method.name]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.x() -> f32 { return 0.0 }
fn main() -> u8 {

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
| 通常 | `[method.name]` | 2 | 4 |

### method-007 selfは変数名に使えない

主根拠：`[method.self-keyword]`。

**ソース：**

```suz
fn main() -> u8 {
    fix self = 1
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
| 通常 | `[src.ident]` | 2 | 9 |

### method-008 メソッドの外にselfはない

主根拠：`[method.self-keyword]`。

**ソース：**

```suz
fn main() -> u8 {
    fix x = self
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
| 通常 | `[method.self-keyword]` | 2 | 13 |

### method-009 読み取りメソッドではselfを書き換えない

主根拠：`[method.self-read]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.read() -> void { self#x = 1.0 }
fn main() -> u8 {

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
| 通常 | `[fn.parameter]` | 2 | 26 |

### method-010 self全体への代入を呼び出し元へ反映する

主根拠：`[method.self-write]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2#reset() -> void { self = Vec2 { x: 0.0, y: 0.0 } }
fn main() -> u8 {
    var p = Vec2 { x: 1.0, y: 2.0 }
    p#reset()
    if p.x != 0.0 || p.y != 0.0 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### method-011 読み取りメソッドを#で呼べない

主根拠：`[method.call]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.length2() -> f32 { return self.x * self.x + self.y * self.y }
fn main() -> u8 {
    var p = Vec2 { x: 1.0, y: 2.0 }
    p#length2()
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
| 通常 | `[method.call]` | 5 | 7 |

### method-012 受け手のフィールドはドットで辿る

主根拠：`[method.call]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2#add(other: Vec2) -> void { self#x += other.x
    self#y += other.y }
struct Holder { pos: Vec2 }
fn main() -> u8 {
    var p = Holder { pos: Vec2 { x: 1.0, y: 2.0 } }
    p#pos#add(Vec2 { x: 1.0, y: 1.0 })
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
| 通常 | `[method.call]` | 7 | 6 |

### method-013 参照を受け手に更新する

主根拠：`[method.call]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2#add(other: Vec2) -> void { self#x += other.x
    self#y += other.y }
fn main() -> u8 {
    fix p = new Vec2 { x: 1.0, y: 2.0 }
    p#add(Vec2 { x: 3.0, y: 4.0 })
    if p.x != 4.0 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### method-014 fixの値に更新メソッドを呼べない

主根拠：`[method.write-target]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2#add(other: Vec2) -> void { self#x += other.x
    self#y += other.y }
fn main() -> u8 {
    fix p = Vec2 { x: 1.0, y: 2.0 }
    p#add(p)
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
| 通常 | `[method.write-target]` | 6 | 5 |

### method-015 通常の仮引数を更新できない

主根拠：`[method.write-target]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2#add(other: Vec2) -> void { self#x += other.x
    self#y += other.y }
fn update(p: Vec2) -> void { p#add(p) }
fn main() -> u8 {

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
| 通常 | `[method.write-target]` | 4 | 30 |

### method-016 更新メソッドの呼び出しはvarの更新に数える

主根拠：`[method.write-effects]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2#add(other: Vec2) -> void { self#x += other.x
    self#y += other.y }
fn main() -> u8 {
    var p = Vec2 { x: 0.0, y: 0.0 }
    p#add(Vec2 { x: 1.0, y: 2.0 })
    if p.y != 2.0 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### method-017 読み取りselfを参照キャプチャーしない

主根拠：`[method.self-capture]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fnshape Action() -> void
fn run(f: fn Action) -> void { f() }
fn Vec2.read() -> void { run(fn [var self]() -> void {}) }
fn main() -> u8 {

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
| 通常 | `[closure.capture-list]` | 4 | 34 |

### method-018 メソッドを関数値にしない

主根拠：`[method.no-fn-value]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.length2() -> f32 { return self.x * self.x + self.y * self.y }
fn main() -> u8 {
    fix f = fn Vec2.length2
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
| 通常 | `[method.no-fn-value]` | 4 | 20 |

### method-019 加算とスカラー倍の演算子メソッド

主根拠：`[operator.method]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.+(other: Vec2) -> Vec2 { return Vec2 { x: self.x + other.x, y: self.y + other.y } }
fn Vec2.*(scale: f32) -> Vec2 { return Vec2 { x: self.x * scale, y: self.y * scale } }
fn main() -> u8 {
    fix p = Vec2 { x: 2.0, y: 4.0 }
    fix q = p * 0.5 + p
    if q.x != 3.0 || q.y != 6.0 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### method-020 比較メソッドの結果はbool

主根拠：`[operator.method]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.<(other: Vec2) -> i32 { return 0 }
fn main() -> u8 {

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
| 通常 | `[operator.method]` | 2 | 4 |

### method-021 演算子メソッドに#を使わない

主根拠：`[operator.method]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2#+(other: Vec2) -> Vec2 { return self }
fn main() -> u8 {

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
| 通常 | `[operator.method]` | 2 | 4 |

### method-022 等値比較のメソッドを定義しない

主根拠：`[operator.no-equality]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.==(other: Vec2) -> bool { return true }
fn main() -> u8 {

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
| 通常 | `[operator.no-equality]` | 2 | 9 |

### method-023 右辺の型で乗算を選ぶ

主根拠：`[operator.overload]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.*(scale: f32) -> Vec2 { return Vec2 { x: self.x * scale, y: self.y * scale } }
fn Vec2.*(other: Vec2) -> f32 { return self.x * other.x + self.y * other.y }
fn main() -> u8 {
    fix p = Vec2 { x: 2.0, y: 4.0 }
    fix q = p * 0.5
    if q.x != 1.0 || p * p != 20.0 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### method-024 同じ右辺型の演算子を重複しない

主根拠：`[operator.overload]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.*(scale: f32) -> Vec2 { return Vec2 { x: self.x * scale, y: self.y * scale } }
fn Vec2.*(scale: f32) -> Vec2 { return Vec2 { x: self.x * scale, y: self.y * scale } }
fn main() -> u8 {

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
| 通常 | `[operator.overload]` | 3 | 4 |

### method-025 同じ右辺型の演算子を重複しない

主根拠：`[operator.overload]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.*(scale: f32) -> Vec2 { return Vec2 { x: self.x * scale, y: self.y * scale } }
fn Vec2.*(scale: f32) -> f32 { return self.x * scale }
fn main() -> u8 {

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
| 通常 | `[operator.overload]` | 3 | 4 |

### method-026 参照は演算子メソッドの受け手にしない

主根拠：`[operator.call]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.+(other: Vec2) -> Vec2 { return Vec2 { x: self.x + other.x, y: self.y + other.y } }
fn main() -> u8 {
    fix p = new Vec2 { x: 1.0, y: 2.0 }
    fix q = p + Vec2 { x: 1.0, y: 2.0 }
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
| 通常 | `[expr.operator-types]` | 5 | 15 |

### method-027 単項マイナスのメソッド

主根拠：`[operator.unary]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.-() -> Vec2 { return Vec2 { x: -self.x, y: -self.y } }
fn main() -> u8 {
    fix p = Vec2 { x: 1.0, y: 2.0 }
    fix q = -p
    if q.x != -1.0 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### method-028 notの結果はbool

主根拠：`[operator.unary]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.not() -> Vec2 { return self }
fn main() -> u8 {

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
| 通常 | `[operator.method]` | 2 | 4 |

### method-029 <の定義から<=を補わない

主根拠：`[operator.independent]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.<(other: Vec2) -> bool { return self.x < other.x }
fn main() -> u8 {
    fix p = Vec2 { x: 1.0, y: 2.0 }
    fix q = p <= p
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
| 通常 | `[operator.overload]` | 5 | 13 |

### method-030 <の定義から>を補わない

主根拠：`[operator.independent]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.<(other: Vec2) -> bool { return self.x < other.x }
fn main() -> u8 {
    fix p = Vec2 { x: 1.0, y: 2.0 }
    fix q = p > p
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
| 通常 | `[operator.overload]` | 5 | 13 |

### method-031 比較連鎖の中央を1回だけ評価する

主根拠：`[operator.chain]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.<(other: Vec2) -> bool { return self.x < other.x }
fn middle(count: mut i32) -> Vec2 { #count += 1
    return Vec2 { x: 2.0, y: 0.0 } }
fn main() -> u8 {
    fix count = new (0 as i32)
    fix a = Vec2 { x: 1.0, y: 0.0 }
    fix c = Vec2 { x: 3.0, y: 0.0 }
    if not (a < middle(count) < c) || .count != 1 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### method-032 成功する演算子メソッドにorを付けない

主根拠：`[operator.arith-boundary]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.*(other: Vec2) -> i32 { return 1 }
fn main() -> u8 {
    fix p = Vec2 { x: 1.0, y: 2.0 }
    fix q = p * p or 0
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
| 通常 | `[operator.arith-boundary]` | 5 | 19 |

### method-033 演算子メソッドで複合代入する

主根拠：`[operator.compound]`。

補助根拠：`[operator.method]`、`[operator.unary]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.+(other: Vec2) -> Vec2 { return Vec2 { x: self.x + other.x, y: self.y + other.y } }
fn Vec2.*(scale: f32) -> Vec2 { return Vec2 { x: self.x * scale, y: self.y * scale } }
struct Phase { raw: u8 }
fn Phase.+%(other: Phase) -> Phase { return Phase { raw: self.raw +% other.raw } }
fn Phase.-%() -> Phase { return Phase { raw: -%self.raw } }
fn main() -> u8 {
    var p = Vec2 { x: 1.0, y: 2.0 }
    p += Vec2 { x: 3.0, y: 4.0 }
    p *= 0.5
    var ph = Phase { raw: 250 }
    ph +%= Phase { raw: 10 }
    if p.x != 2.0 || p.y != 3.0 || ph.raw != 4 || (-%ph).raw != 252 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### method-034 結果の型が違う演算子では複合代入しない

主根拠：`[operator.compound]`。

**ソース：**

```suz
struct Vec2 { x: f32, y: f32 }
fn Vec2.*(other: Vec2) -> f32 { return 1.0 }
fn main() -> u8 {
    var p = Vec2 { x: 1.0, y: 2.0 }
    p *= p
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
| 通常 | `[operator.compound]` | 5 | 7 |
