# 5. 型

[目次と書式](../spec-cases.md)

### type-001 整数でboolの変数を初期化する

主根拠：`[type.bool]`。

整数の1をtrueとして扱う暗黙の変換は行わない。

**ソース：**

```suz
fn main() -> u8 {
    fix flag: bool = 1
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
| 通常 | `[type.expected]` | 2 | 22 |

### type-002 boolをu8の戻り値にする

主根拠：`[type.bool]`。

trueを整数の1として返す暗黙の変換は行わない。

**ソース：**

```suz
fn main() -> u8 {
    return true
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.expected]` | 2 | 12 |

### type-003 voidを返す呼び出しで変数を初期化する

主根拠：`[type.void]`。

**ソース：**

```suz
fn action() -> void {}

fn main() -> u8 {
    fix value = action()
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

型注釈を省略しても、voidを変数の型にはできない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.void]` | 4 | 9 |

### type-004 仮引数の型にvoidを使う

主根拠：`[type.void]`。

**ソース：**

```suz
fn action(value: void) -> void {}

fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

引数を取らない関数は空の丸括弧で宣言する。void型の仮引数は宣言できない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.void]` | 1 | 11 |

### type-005 構造体のフィールドの型にvoidを使う

主根拠：`[type.void]`。

**ソース：**

```suz
struct Item { value: void }

fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.void]` | 1 | 15 |

### type-006 型の決まった整数同士の暗黙変換

主根拠：`[type.numeric-conversion]`。

補助根拠：`[expr.operator-types]`。

**ソース：**

```suz
fn main() -> u8 {
    fix left: u8 = 1
    fix right: i32 = 2
    fix total = left + right
    return ifexpr total == 3 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

u8とi32の全範囲を含むi32へ、正確な和3を格納する。

### type-007 同型の推論とビット演算

主根拠：`[type.propagation]`。

補助根拠：`[type.mixed-literals]`、`[expr.bitwise]`。

**ソース：**

```suz
fn main() -> u8 {
    fix sum: u8 = 1 + 2
    fix bits: u8 = ~0
    fix mixed = 1 + 0.5
    return ifexpr sum == 3 && bits == 255 && mixed == 1.5 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### type-008 シフトの左右に異なる整数型を使う

主根拠：`[type.propagation-boundary]`。

補助根拠：`[expr.operator-types]`。

**ソース：**

```suz
fn main() -> u8 {
    fix count: u64 = 3
    fix result: u8 = 1 << count
    return result
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 8 | 0 |

左辺と結果はu8、シフト数はu64。シフト数に左辺と同じ型は要求しない。

### type-009 キャスト先の型は変換前の加算へ伝わらない

主根拠：`[type.propagation-boundary]`。

補助根拠：`[type.literal-default]`、`[const.cast-check]`。

**ソース：**

```suz
fn main() -> u8 {
    fix result = (255 + 1) as u8 or 7
    return result
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 7 | 0 |

255 + 1は既定のi64で計算して256になる。その後のu8への変換が失敗し、代替値7を使う。

### type-010 分岐の順序によらず既知の型をリテラルへ伝える

主根拠：`[type.branch-inference]`。

補助根拠：`[type.inference-order]`。

**ソース：**

```suz
fn add(left: u8, right: u8) -> u8 {
    return left + right
}

fn main() -> u8 {
    fix known: u8 = 2
    fix first = ifexpr true { 1 } else { known }
    fix second = ifexpr true { known } else { 1 }
    return add(first, second)
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 3 | 0 |

型注釈のないfirstとsecondもu8になる。型の決まった値が後ろの分岐にあっても同じ。

### type-011 分岐の整数型を値を失わない型へそろえる

主根拠：`[type.branch-inference]`。

補助根拠：`[type.inference-order]`。

**ソース：**

```suz
fn main() -> u8 {
    fix small: u8 = 1
    fix large: u16 = 1
    fix result = ifexpr true { small } else { large }
    return ifexpr result == 1 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

u8とu16の全範囲を含むu16を分岐の結果型にする。選ばれない分岐も型検査する。

### type-012 既定型で宣言した変数を別の型として返す

主根拠：`[type.literal-default]`。

補助根拠：`[type.numeric-conversion]`、`[type.expected]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = 1
    return value
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

valueは初期化時にi64と決まる。returnの要求するu8へ暗黙変換しない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.expected]` | 3 | 12 |

### type-013 構造体を宣言して値をコピーする

主根拠：`[type.value-copy]`。

補助根拠：`[struct.literal]`、`[struct.shorthand]`、`[struct.shorthand-name]`、`[struct.shorthand-rules]`、`[struct.with]`。

**ソース：**

```suz
struct Point { x: i32 }

fn main() -> u8 {
    fix x: i32 = 1
    fix original = Point { x }
    var copied = original
    copied.x = 2
    fix updated = original with { x: 3 }
    return ifexpr original.x == 1 && copied.x == 2 && updated.x == 3 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

`Point { x }` は `Point { x: x }` と同じ意味である。structで宣言した型はコピー可能。コピー先のフィールドを変更しても元の値は変わらない。`with`式でも、元の値を変更せずにフィールドの値が異なるコピーを作れる。

### type-014 列挙型を宣言して値をコピーする

主根拠：`[type.value-copy]`。

補助根拠：`[enum.base]`。

**ソース：**

```suz
enum Wave: u8 { Sine, Saw }

fn main() -> u8 {
    fix original = Wave.Sine
    var copied = original
    copied = Wave.Saw
    return ifexpr original == Wave.Sine && copied == Wave.Saw { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

enumで宣言した型はコピー可能。コピー先に別の列挙子を代入しても元の値は変わらない。

### type-015 入れ子の構造体をコピーしてフィールドを変更する

主根拠：`[type.value-copy]`。

補助根拠：`[var.never-reassigned]`、`[assign.target]`、`[fn.call]`。

**ソース：**

```suz
struct Point { x: i32 }
struct Box { point: Point }

fn changed(original: Box) -> Box {
    var copy = original
    copy.point.x = 9
    return copy
}

fn main() -> u8 {
    fix first = Box { point: Point { x: 1 } }
    fix second = changed(first)
    return ifexpr first.point.x == 1 && second.point.x == 9 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

引数をvarへコピーして、その入れ子のフィールドを変更する。呼び出し元の値は変わらない。フィールドへの代入はvarの再代入として数える。

### type-016 宣言が違う構造体

主根拠：`[type.identity]`。

補助根拠：`[type.expected]`。

**ソース：**

```suz
struct Position { x: i32 }
struct Velocity { x: i32 }

fn main() -> u8 {
    fix p = Position { x: 1 }
    fix v: Velocity = p
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

PositionとVelocityは異なる型。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.expected]` | 6 | 23 |

### type-017 構成が同じでも異なるenumは代入できない

主根拠：`[type.identity]`。

補助根拠：`[type.expected]`。

**ソース：**

```suz
enum First: u8 { A, B }
enum Second: u8 { A, B }

fn main() -> u8 {
    fix value: Second = First.A
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

基底型と列挙子名が同じでも、別の宣言によるenumは異なる型。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.expected]` | 5 | 25 |

### type-018 フィールド名は外側の名前を隠蔽しない

主根拠：`[struct.field]`。

**ソース：**

```suz
fn x() -> u8 { return 3 }
struct Point { x: u8 }

fn main() -> u8 {
    fix point = Point { x: x() }
    return point.x
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 3 | 0 |

関数xと構造体のフィールドxは共存できる。初期値では関数xを呼び出す。

### type-019 空の構造体

主根拠：`[struct.nonempty]`。

**ソース：**

```suz
struct Empty {}
fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

フィールドが1つ以上必要。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[struct.nonempty]` | 1 | 8 |

### type-020 構造体の初期化漏れ

主根拠：`[struct.literal]`。

**ソース：**

```suz
struct Pair { x: i32, y: i32 }

fn main() -> u8 {
    fix value = Pair { x: 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

yの初期値がない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[struct.literal]` | 4 | 17 |

### type-021 構造体リテラルで同じフィールドを2回指定する

主根拠：`[struct.literal]`。

**ソース：**

```suz
struct Point { x: i32 }

fn main() -> u8 {
    fix value = Point { x: 1, x: 2 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

xを重複して指定している。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[struct.literal]` | 4 | 31 |

### type-022 構造体は記述順に初期化し、コピーで渡す

主根拠：`[struct.eval-order]`。

補助根拠：`[struct.access]`、`[fn.call]`。

**ソース：**

```suz
struct Pair { x: i32, y: i32 }

fn mark(n: i32) -> i32 {
    log info `${n}`
    return n
}

fn change(original: Pair) -> Pair {
    var copy = original
    copy.x = 9
    return copy
}

fn main() -> u8 {
    fix first = Pair { y: mark(2), x: mark(1) }
    fix second = change(first)
    return ifexpr first.x == 1 && second.x == 9 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info type-022.suz:4: 2
info type-022.suz:4: 1
```

### type-023 構造体が互いをフィールドに持つ

主根拠：`[struct.recursion]`。

**ソース：**

```suz
struct First { second: Second }
struct Second { first: First }

fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

FirstはSecondを介して自分自身を値として含むため、間接的な再帰になる。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[struct.recursion]` | 2 | 17 |

### type-024 enumの基底型を省略する

主根拠：`[enum.base]`。

**ソース：**

```suz
enum Wave { Sine, Saw }

fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[enum.base]` | 1 | 11 |

### type-025 enumの基底型に浮動小数点型を指定する

主根拠：`[enum.base]`。

**ソース：**

```suz
enum Wave: f64 { Sine, Saw }

fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

基底型には整数型を指定する。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[enum.base]` | 1 | 12 |

### type-026 空の列挙型

主根拠：`[enum.nonempty]`。

**ソース：**

```suz
enum Empty: u8 {}
fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

列挙子が1つ以上必要。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[enum.nonempty]` | 1 | 6 |

### type-027 同じenum内で列挙子名を重複させる

主根拠：`[enum.variant]`。

**ソース：**

```suz
enum Wave: u8 { Sine, Sine }

fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[enum.variant]` | 1 | 23 |

### type-028 列挙子の整数値を明示的に指定する

主根拠：`[enum.variant]`。

**ソース：**

```suz
enum Wave: u8 { Sine = 1, Saw = 2 }

fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

列挙子の整数値は宣言順に付く。値を指定する構文はない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[enum.variant]` | 1 | 22 |

### type-029 enumの自動採番が基底型の最大値に一致する

主根拠：`[enum.variant]`。

補助根拠：`[expr.enum-conversion]`。

**ソース：**

```suz
enum Code: i8 {
    V000, V001, V002, V003, V004, V005, V006, V007,
    V008, V009, V010, V011, V012, V013, V014, V015,
    V016, V017, V018, V019, V020, V021, V022, V023,
    V024, V025, V026, V027, V028, V029, V030, V031,
    V032, V033, V034, V035, V036, V037, V038, V039,
    V040, V041, V042, V043, V044, V045, V046, V047,
    V048, V049, V050, V051, V052, V053, V054, V055,
    V056, V057, V058, V059, V060, V061, V062, V063,
    V064, V065, V066, V067, V068, V069, V070, V071,
    V072, V073, V074, V075, V076, V077, V078, V079,
    V080, V081, V082, V083, V084, V085, V086, V087,
    V088, V089, V090, V091, V092, V093, V094, V095,
    V096, V097, V098, V099, V100, V101, V102, V103,
    V104, V105, V106, V107, V108, V109, V110, V111,
    V112, V113, V114, V115, V116, V117, V118, V119,
    V120, V121, V122, V123, V124, V125, V126, V127,
}

fn main() -> u8 {
    fix last = Code.V127 as i8
    return ifexpr last == 127 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

128個の列挙子に0から127までを割り当てる。最後のV127はi8の最大値に収まる。

### type-030 enumの自動採番が基底型の最大値を超える

主根拠：`[enum.variant]`。

**ソース：**

```suz
enum Code: i8 {
    V000, V001, V002, V003, V004, V005, V006, V007,
    V008, V009, V010, V011, V012, V013, V014, V015,
    V016, V017, V018, V019, V020, V021, V022, V023,
    V024, V025, V026, V027, V028, V029, V030, V031,
    V032, V033, V034, V035, V036, V037, V038, V039,
    V040, V041, V042, V043, V044, V045, V046, V047,
    V048, V049, V050, V051, V052, V053, V054, V055,
    V056, V057, V058, V059, V060, V061, V062, V063,
    V064, V065, V066, V067, V068, V069, V070, V071,
    V072, V073, V074, V075, V076, V077, V078, V079,
    V080, V081, V082, V083, V084, V085, V086, V087,
    V088, V089, V090, V091, V092, V093, V094, V095,
    V096, V097, V098, V099, V100, V101, V102, V103,
    V104, V105, V106, V107, V108, V109, V110, V111,
    V112, V113, V114, V115, V116, V117, V118, V119,
    V120, V121, V122, V123, V124, V125, V126, V127,
    V128,
}

fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

129個目の列挙子V128に割り当てる128はi8に収まらない。列挙子を使用するかどうかにかかわらず、宣言をエラーにする。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[enum.variant]` | 18 | 5 |

### type-031 列挙子名のスコープと宣言順の整数値

主根拠：`[enum.access]`。

補助根拠：`[enum.variant]`。

**ソース：**

```suz
fix FIRST: u8 = 9
enum Left: u8 { FIRST, Second, Third }
enum Right: u8 { Third, Second, FIRST }

fn main() -> u8 {
    fix a = Left.FIRST as u8
    fix b = Left.Second as u8
    fix c = Left.Third as u8
    fix d = Right.FIRST as u8
    return ifexpr FIRST == 9 && a == 0 && b == 1 && c == 2 && d == 2 { 0 } else { 1 }
}
```


**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

列挙子名は各enumの中で区別する。外側の定数名や別のenumの列挙子名と同じでもよい。整数値は各enumで0から始まる。

### type-032 charのフィールドをコピーし、コピー先だけを変更する

主根拠：`[char.type]`。

**ソース：**

```suz
struct Letter { ch: char }
fn echo(ch: char) -> char { return ch }
fn main() -> u8 {
    fix original = Letter { ch: 'a' }
    var copy = original
    copy.ch = echo('b')
    log info `${original.ch} ${copy.ch}`
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info type-032.suz:7: a b
```

### type-033 引数・戻り値・フィールド・分岐の拡大変換

主根拠：`[conv.widen-places]`。

**ソース：**

```suz
struct Point { x: i32 }
fn widen(value: u16) -> f64 { return value }
fn main() -> u8 {
    fix small: u8 = 7
    fix negative: i8 = -1
    fix point = Point { x: small }
    fix joined = ifexpr false { small } else { negative }
    fix result: f64 = widen(small)
    return ifexpr point.x == 7 && joined == -1 && result == 7.0 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

u8をu16の実引数、i32のフィールドへ広げ、u16の戻り値をf64へ広げる。分岐のu8とi8はi16にそろえる。
### type-034 intを8種類の整数型で使う

主根拠：`[num.int]`。

各呼び出しの `int` は、引数と受け取り先に指定した整数型になる。

**ソース：**

```suz
fn identity(value: int) -> int {
    return value
}
fn main() -> u8 {
    fix value_i8: i8 = 1
    fix result_i8: i8 = identity(value_i8)
    if result_i8 != 1 { return 1 }
    fix value_i16: i16 = 1
    fix result_i16: i16 = identity(value_i16)
    if result_i16 != 1 { return 1 }
    fix value_i32: i32 = 1
    fix result_i32: i32 = identity(value_i32)
    if result_i32 != 1 { return 1 }
    fix value_i64: i64 = 1
    fix result_i64: i64 = identity(value_i64)
    if result_i64 != 1 { return 1 }
    fix value_u8: u8 = 1
    fix result_u8: u8 = identity(value_u8)
    if result_u8 != 1 { return 1 }
    fix value_u16: u16 = 1
    fix result_u16: u16 = identity(value_u16)
    if result_u16 != 1 { return 1 }
    fix value_u32: u32 = 1
    fix result_u32: u32 = identity(value_u32)
    if result_u32 != 1 { return 1 }
    fix value_u64: u64 = 1
    fix result_u64: u64 = identity(value_u64)
    if result_u64 != 1 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### type-035 floatをf32とf64で使う

主根拠：`[num.float]`。

`float` は呼び出しごとに `f32` または `f64` になる。`f32` では加算結果が丸められ、`f64` では1増える。

**ソース：**

```suz
fn increment(value: float) -> float {
    return value + 1.0
}
fn main() -> u8 {
    fix narrow: f32 = 16777216.0
    fix wide: f64 = 16777216.0
    fix a = increment(narrow)
    fix b = increment(wide)
    return ifexpr a == 16777216.0 && b == 16777217.0 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |
### type-036 floatを関数名に使う

主根拠：`[num.reserved]`。

`float` は組み込み型名のため、関数名には使えない。

**ソース：**

```suz
fn float() -> u8 {
    return 0
}
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
| 通常 | `[src.ident]` | 1 | 4 |

### type-037 通常の関数のローカル変数にintを指定する

主根拠：`[num.places]`。

`main` は汎用関数ではないため、ローカル変数の型に `int` を指定できない。

**ソース：**

```suz
fn main() -> u8 {
    fix value: int = 0
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
| 通常 | `[num.places]` | 2 | 9 |
