# 型

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
    copied#x = 2
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
    copy.point#x = 9
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
    log info "${n}"
    return n
}

fn change(original: Pair) -> Pair {
    var copy = original
    copy#x = 9
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
    copy#ch = echo('b')
    log info "${original.ch} ${copy.ch}"
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

主根拠：`[src.type-names]`。

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

### type-038 値域制限型の宣言・構造体・関数・配列要素

主根拠：`[range.decl]`。
補助根拠：`[range.bounds]`、`[range.places]`。

半開区間 `0..<256`（`u8` に収まる `0..=255`）や 1 値型 `3..=3` を含む値域制限型を宣言し、構造体フィールド・仮引数・戻り値・配列要素として利用できる。

**ソース：**

```suz
range ByteValue: u8 = 0..<256
range Three: u8 = 3..=3

struct Packet {
    value: ByteValue,
    tag: Three,
}

fn pass(v: ByteValue) -> ByteValue {
    return v
}

fn main() -> u8 {
    fix p = Packet { value: 255, tag: 3 }
    fix arr: #[ByteValue] = new #[0; 2]
    arr#[0] = pass(p.value)
    if arr.[0] == 255 {
        return p.tag
    }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 3 | 0 |

### type-039 値域制限型の空区間を拒否する

主根拠：`[range.bounds]`。

半開区間 `0..<0` には含まれる値が 1 つもないためコンパイルエラーとなる。

**ソース：**

```suz
range Empty: u8 = 0..<0

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
| 通常 | `[range.bounds]` | 1 | 7 |

### type-040 値域制限型の逆転区間を拒否する

主根拠：`[range.bounds]`。

下限が上限より大きい区間 `5..=3` はコンパイルエラーとなる。

**ソース：**

```suz
range Reversed: u8 = 5..=3

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
| 通常 | `[range.bounds]` | 1 | 7 |

### type-041 値域制限型の基底型超過を拒否する

主根拠：`[range.bounds]`。

`0..=256` は基底型 `u8` の範囲を超えるためコンパイルエラーとなる。

**ソース：**

```suz
range Over: u8 = 0..=256

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
| 通常 | `[range.bounds]` | 1 | 7 |

### type-042 同範囲の別名値域型の取り違えを拒否する

主根拠：`[type.expected]`。
補助根拠：`[range.widen]`。

同じ範囲 `0..=127` を持つ `Note` と `Velocity` でも別の名前付き型として区別し、暗黙に受け渡せない。

**ソース：**

```suz
range Note: u8 = 0..=127
range Velocity: u8 = 0..=127

fn play(n: Note) -> u8 {
    return 0
}

fn main() -> u8 {
    fix v: Velocity = 64
    return play(v)
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.expected]` | 10 | 17 |

### type-043 異なる値域型の直接比較を拒否する

主根拠：`[range.compare]`。

同じ範囲を持つ `Note` と `Velocity` でも直接比較できない。

**ソース：**

```suz
range Note: u8 = 0..=127
range Velocity: u8 = 0..=127

fn main() -> u8 {
    fix n: Note = 60
    fix v: Velocity = 60
    if n == v {
        return 1
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
| 通常 | `[range.compare]` | 7 | 10 |

### type-044 値域制限型のmatchで範囲外パターンを拒否する

主根拠：`[lit.range]`。
補助根拠：`[range.bounds]`、`[range.places]`、`[match.pattern]`。

値域制限型を対象とする `match` のパターンには対象と同じ値域制限型を要求するため、範囲外リテラルは拒否する。

**ソース：**

```suz
range Bit: u8 = 0..=1

fn main() -> u8 {
    fix b: Bit = 0
    match b {
        0 => { return 0 }
        2 => { return 1 }
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
| 通常 | `[lit.range]` | 7 | 9 |

### type-045 値域制限型のmatchで全値列挙時も_を必須とする

主根拠：`[match.exhaustive]`。
補助根拠：`[range.places]`。

値域制限型の `match` ではすべての値を列挙しても末尾の `_` が必須である。

**ソース：**

```suz
range Bit: u8 = 0..=1

fn main() -> u8 {
    fix b: Bit = 0
    match b {
        0 => { return 0 }
        1 => { return 1 }
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
| 通常 | `[match.exhaustive]` | 5 | 5 |

### type-046 値域制限型の変換・算術・文字列変換・非零除算・match・埋め込み

主根拠：`[range.cast]`。
補助根拠：`[range.widen]`、`[range.cast-float-sat]`、`[range.cast-text]`、`[range.arith]`、`[range.places]`、`[conv.widen]`、`[arith.width]`、`[arith.store]`、`[expr.cast-risk]`。

値域制限型から `u8`・`len_t`・`f64` への暗黙変換、包含が静的に分かる無検査 `as`、`(0..=127) + (0..=127)` の `u8` 格納、`x %% 24` の `Hour`（`0..=23`）への無失敗格納、`as|` の上下限飽和（整数および浮動小数点 `inf` / `-inf`）、文字列からの `as` / `as|`、非零除算の格納失敗 `or`、テンプレート埋め込みと `match` / `matchexpr` を確認する。

**ソース：**

```suz
range Note: u8 = 0..=127
range Velocity: u8 = 0..=127
range Wide: u16 = 0..=1000
range Divisor: u16 = 1..=1000
range Hour: u8 = 0..=23

fn quotient(x: u16, d: Divisor) -> u8 {
    return x / d or 255
}

fn wrap_hour(x: i32) -> Hour {
    return x %% 24
}

fn main() -> u8 {
    fix n: Note = 100
    fix as_u8: u8 = n
    fix as_len: len_t = n
    fix as_f64: f64 = n
    fix v: Velocity = n as Velocity
    fix w: Wide = as_u8 as Wide
    fix sum = n + n
    fix clamped: Note = 200 as| Note
    fix pos_inf: f64 = inf
    fix neg_inf: f64 = -inf
    fix finf_hi: Note = pos_inf as| Note or 0
    fix finf_lo: Divisor = neg_inf as| Divisor or 50
    fix parsed: Note = "127" as Note or 0
    fix sat_hi: Note = "1000" as| Note or 0
    fix sat_lo: Note = "-5" as| Note or 1
    fix d: Divisor = 1
    fix q_ok = quotient(200, d)
    fix q_ovf = quotient(300, d)
    fix h: Hour = wrap_hour(-1)
    fix s: symbol = "${n}:${h}"
    fix m = matchexpr h {
        23 => 1
        _ => 0
    }
    if as_u8 == 100 && as_len == 100 && as_f64 == 100.0 && v == 100 && w == 100 && sum == 200 && clamped == 127 && finf_hi == 127 && finf_lo == 1 && parsed == 127 && sat_hi == 127 && sat_lo == 0 && q_ok == 200 && q_ovf == 255 && h == 23 && s == "100:23" && m == 1 {
        return 0
    }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### type-047 0を含まない値域型への浮動小数点as|でor省略を拒否する

主根拠：`[range.cast-float-sat]`。

`Divisor`（`1..=1000`）は `0` を含まないため、浮動小数点からの `as|` で NaN ハンドラ `or` を省略するとコンパイルエラーとなる。

**ソース：**

```suz
range Divisor: u16 = 1..=1000

fn main() -> u8 {
    fix x: f64 = nan
    fix d: Divisor = x as| Divisor
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
| 通常 | `[range.cast-float-sat]` | 5 | 24 |

### type-048 値域制限型へのラップ算術を拒否する

主根拠：`[range.prohibited-ops]`。

値域制限型はラップ算術（`+%` 等）の対象外である。

**ソース：**

```suz
range Note: u8 = 0..=127

fn main() -> u8 {
    fix n: Note = 60
    fix m = n +% n
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
| 通常 | `[range.prohibited-ops]` | 5 | 15 |

### type-049 値域制限型へのビット演算を拒否する

主根拠：`[range.prohibited-ops]`。

値域制限型を直接のオペランドとするビット演算は対象外である。

**ソース：**

```suz
range Note: u8 = 0..=127

fn main() -> u8 {
    fix n: Note = 60
    fix m = ~n
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
| 通常 | `[range.prohibited-ops]` | 5 | 13 |

### type-050 値域制限型へのas%を拒否する

主根拠：`[range.cast]`。

値域制限型は `as%` の変換先にできない。

**ソース：**

```suz
range Note: u8 = 0..=127

fn main() -> u8 {
    fix x: u16 = 200
    fix n: Note = x as% Note
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
| 通常 | `[range.cast]` | 5 | 21 |

### type-051 匿名汎用intと明示的な型引数Tで値域制限型を扱う

主根拠：`[range.generic]`。
補助根拠：`[num.int]`、`[num.fn-resolve]`、`[num.fn-check-judge]`。

`min(note1, note2)` は `Note` を返し、`fix r: u8 = twice(note)` は `u8` で具体化、要求型なしの `twice(note)` は `Note` で具体化される。無関係な `range Divisor: u16 = 1..=1000` があっても `zero()` の定義時検査と `u8` での呼び出しに影響しない。明示的な型引数 `T` も値域制限型をそのまま保持する。

**ソース：**

```suz
range Note: u8 = 0..=127
range Divisor: u16 = 1..=1000

fn min(a: int, b: int) -> int {
    return ifexpr a < b { a } else { b }
}

fn twice(x: int) -> int {
    return x + x
}

fn zero() -> int {
    return 0
}

fn keep[T](x: T) -> T {
    return x
}

fn main() -> u8 {
    fix n1: Note = 63
    fix n2: Note = 40
    fix m: Note = min(n1, n2)
    fix n_ok: Note = twice(n1)
    fix big: Note = 100
    fix as_u8: u8 = twice(big)
    fix z: u8 = zero()
    fix kept: Note = keep(n1)
    if m == 40 && n_ok == 126 && as_u8 == 200 && z == 0 && kept == 63 {
        return 0
    }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### type-052 匿名汎用intで通常整数候補のない異なる値域型の混在を拒否する

主根拠：`[num.fn-resolve]`。
補助根拠：`[range.generic]`。

`Note` と `Velocity` のように異なる値域制限型だけが候補にある場合、`[num.fn-resolve]` で拒否する。

**ソース：**

```suz
range Note: u8 = 0..=127
range Velocity: u8 = 0..=127

fn min(a: int, b: int) -> int {
    return ifexpr a < b { a } else { b }
}

fn main() -> u8 {
    fix n: Note = 60
    fix v: Velocity = 80
    fix m = min(n, v)
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
| 通常 | `[num.fn-resolve]` | 11 | 13 |

### type-053 匿名汎用intを0を含まない値域型で具体化したときリテラル不適合を拒否する

主根拠：`[num.fn-body]`。
補助根拠：`[range.generic]`、`[lit.range]`。

`fn zero() -> int { return 0 }` を `Divisor`（`1..=1000`）で具体化すると、`0` が `Divisor` の範囲外であるためコンパイルエラーとなる。

**ソース：**

```suz
range Divisor: u16 = 1..=1000

fn zero() -> int {
    return 0
}

fn main() -> u8 {
    fix d: Divisor = zero()
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
| 通常 | `[num.fn-body]` | 8 | 22 |

### type-054 匿名汎用intで値域型へ具体化した算術の結果超過は実行時にパニックする

主根拠：`[range.generic]`。
補助根拠：`[arith.store]`、`[rt.int-overflow]`。

`twice(note)` を `Note`（`0..=127`）で具体化したとき、`64 + 64 = 128` はコンパイルエラーとせず実行時の格納失敗としてパニックする。

**ソース：**

```suz
range Note: u8 = 0..=127

fn twice(x: int) -> int {
    return x + x
}

fn main() -> u8 {
    fix n: Note = 64
    fix r = twice(n)
    return r
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | パニック | 101 | 0 |

**パニック診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[rt.int-overflow]` | 4 | 14 |

### type-055 値域制限型フィールドを持つ構造体への文字列変換で値域を照合する

主根拠：`[range.cast-text]`。
補助根拠：`[symbol.cast]`、`[text.cast-view]`。

値域制限型フィールドを持つ構造体へ `symbol`・`text` から `as` で変換すると、範囲内は成功し、範囲外（`200`）は失敗して `or` へ進む。

**ソース：**

```suz
range Note: u8 = 0..=127

struct Pk {
    n: Note,
}

fn main() -> u8 {
    fix ok_sym: Pk = "Pk { n: 5 }" as Pk or Pk { n: 9 }
    fix bad_sym: Pk = "Pk { n: 200 }" as Pk or Pk { n: 9 }
    fix t_ok: text = "Pk { n: 127 }"
    fix t_bad: text = "Pk { n: 128 }"
    fix ok_txt = t_ok as Pk or Pk { n: 0 }
    fix bad_txt = t_bad as Pk or Pk { n: 7 }
    if ok_sym.n == 5 && bad_sym.n == 9 && ok_txt.n == 127 && bad_txt.n == 7 {
        return 0
    }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### type-056 通常整数型の変数から値域制限型への暗黙変換を拒否する

主根拠：`[type.expected]`。
補助根拠：`[range.widen]`。

`u8` の全範囲が `Wide`（`0..=1000`）に含まれていても、名前付きの値域制限型へは暗黙変換できない。

**ソース：**

```suz
range Wide: u16 = 0..=1000

fn main() -> u8 {
    fix x: u8 = 10
    fix w: Wide = x
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
| 通常 | `[type.expected]` | 5 | 19 |

### type-057 文字列から値域制限型へのas%を拒否する

主根拠：`[range.cast]`。
補助根拠：`[range.cast-text]`。

`symbol`・`text` からも値域制限型への `as%` は対象外である。

**ソース：**

```suz
range Note: u8 = 0..=127

fn main() -> u8 {
    fix n: Note = "10" as% Note or 0
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
| 通常 | `[range.cast]` | 4 | 24 |

### type-058 値域制限型への飽和算術を拒否する

主根拠：`[range.prohibited-ops]`。

値域制限型は飽和算術（`+|` 等）の対象外である。

**ソース：**

```suz
range Note: u8 = 0..=127

fn main() -> u8 {
    fix n: Note = 60
    fix m = n +| n
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
| 通常 | `[range.prohibited-ops]` | 5 | 15 |

### type-059 値域制限型を左辺とするシフトを拒否する

主根拠：`[range.prohibited-ops]`。

値域制限型はシフトの左オペランドにできない。

**ソース：**

```suz
range Note: u8 = 0..=127

fn main() -> u8 {
    fix n: Note = 1
    fix m = n << 1
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
| 通常 | `[range.prohibited-ops]` | 5 | 15 |

### type-060 値域制限型の基底型にlen_tを指定すると拒否する

主根拠：`[range.decl]`。

値域制限型の基底型には固定幅整数型のみを指定でき、`len_t` は使えない。

**ソース：**

```suz
range Bad: len_t = 0..=10

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
| 通常 | `[range.decl]` | 1 | 12 |

### type-061 値域制限型の基底型に値域制限型を指定すると拒否する

主根拠：`[range.decl]`。

値域制限型の基底型に別の値域制限型は使えない。

**ソース：**

```suz
range Note: u8 = 0..=127
range Sub: Note = 0..=10

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
| 通常 | `[range.decl]` | 2 | 12 |

### type-062 extern関数の引数に値域制限型を指定すると拒否する

主根拠：`[ffi.type]`。
補助根拠：`[range.places]`。

値域制限型は `extern` 関数の引数・戻り値に直接使えない。

**ソース：**

```suz
range Note: u8 = 0..=127

extern "stdint.h" {
    fn f(n: Note) -> u8
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
| 通常 | `[ffi.type]` | 4 | 8 |

### type-063 値域制限型の範囲だけで決まる比較で定数比較警告を出す

主根拠：`[range.compare]`。
補助根拠：`[diagnostic.constant-comparison]`。

`Channel`（`0..<16`）に対する `c < 16` は常に真であるため、通常設定でも `[diagnostic.constant-comparison]` を警告する。

**ソース：**

```suz
range Channel: u8 = 0..<16

fn check(c: Channel) -> u8 {
    if c < 16 {
        return 0
    }
    return 1
}

fn main() -> u8 {
    fix c: Channel = 3
    return check(c)
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 1 |

**警告：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[diagnostic.constant-comparison]` | 4 | 10 |
