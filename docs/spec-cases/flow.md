# 8. 関数と制御

[目次と書式](../spec-cases.md)

### flow-001 関数の戻り値型を省略する

主根拠：`[fn.return-type]`。

**ソース：**

```suz
fn action() {}

fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

値を返さない関数も、戻り値型を-> voidと明記する必要がある。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[fn.return-type]` | 1 | 13 |

### flow-002 仮引数の入れ子のフィールドを変更する

主根拠：`[fn.parameter]`。

補助根拠：`[assign.target]`。

**ソース：**

```suz
struct Point { x: i32 }
struct Box { point: Point }

fn change(value: Box) -> void {
    value.point.x = 9
}

fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

仮引数のフィールドは、入れ子であっても変更できない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[fn.parameter]` | 5 | 5 |

### flow-003 仮引数と同名のローカル変数

主根拠：`[fn.parameter-scope]`。

**ソース：**

```suz
fn use(value: i32) -> void {
    fix value = 1
}

fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

valueが仮引数名と重複する。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[fn.parameter-scope]` | 2 | 9 |

### flow-004 関数呼び出しの引数が不足する

主根拠：`[fn.call]`。

**ソース：**

```suz
fn sum(left: u8, right: u8) -> u8 {
    return left + right
}

fn main() -> u8 {
    return sum(1)
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

sumには2つの実引数が必要。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[fn.call]` | 6 | 12 |

### flow-005 関数の実引数と仮引数の型が異なる

主根拠：`[fn.call]`。

補助根拠：`[type.numeric-conversion]`。

**ソース：**

```suz
fn identity(input: u8) -> u8 {
    return input
}

fn main() -> u8 {
    fix value: u16 = 1
    return identity(value)
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

valueの型はu16であり、u8の仮引数へ暗黙変換しない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.expected]` | 7 | 21 |

### flow-006 関数の相互再帰

主根拠：`[fn.recursion]`。

補助根拠：`[decl.top-level]`、`[fn.call]`。

**ソース：**

```suz
fn is_even(n: u8) -> bool {
    if n == 0 {
        return true
    }
    return is_odd(n - 1)
}

fn is_odd(n: u8) -> bool {
    if n == 0 {
        return false
    }
    return is_even(n - 1)
}

fn main() -> u8 {
    return ifexpr is_even(4) && is_odd(3) { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

is_evenとis_oddが互いを呼び出す。0で再帰を止める。

### flow-007 void関数はreturnと末尾到達の両方で戻れる

主根拠：`[fn.return]`。

補助根拠：`[fn.fallthrough]`。

**ソース：**

```suz
fn visit(stop: bool) -> void {
    if stop {
        return
    }
}

fn main() -> u8 {
    visit(true)
    visit(false)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

stopがtrueならreturnで戻り、falseなら本体末尾に到達して戻る。

### flow-008 void関数のreturnに値を付ける

主根拠：`[fn.return]`。

**ソース：**

```suz
fn action() -> void {
    return 1
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
| 通常 | `[fn.return]` | 2 | 12 |

### flow-009 値を返す関数で式のないreturnを書く

主根拠：`[fn.return]`。

**ソース：**

```suz
fn choose(flag: bool) -> u8 {
    if flag {
        return
    }
    return 1
}

fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

ほかの経路にreturn式があっても、値のないreturnはu8関数では使えない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[fn.return]` | 3 | 15 |

### flow-010 非void関数にreturn式がない

主根拠：`[fn.value-return-required]`。

**ソース：**

```suz
fn wait_forever() -> i32 {
    loop {}
}
fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

末尾へ到達しなくても、return式が少なくとも1つ必要。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[fn.value-return-required]` | 3 | 1 |

### flow-011 関数末尾へ到達する経路

主根拠：`[fn.fallthrough]`。

補助根拠：`[fn.flow-analysis]`。

**ソース：**

```suz
fn choose(flag: bool) -> u8 {
    if flag {
        return 1
    }
}
fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

elseがなく、末尾へ進みうる。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[fn.fallthrough]` | 5 | 1 |

### flow-059 revの内部だけでreturnして関数末尾に到達する

主根拠：`[fn.fallthrough]`。

**ソース：**

```suz
fn main() -> u8 {
    rev i in 0..<1 { return 3 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[fn.fallthrough]` | 3 | 1 |

### flow-012 if trueでもelseがなければ末尾へ進みうる

主根拠：`[fn.fallthrough]`。

条件の値ではなく構文で判定する。returnは存在するが、最後のelseがないため関数末尾への到達を防げない。

**ソース：**

```suz
fn main() -> u8 {
    if true { return 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[fn.fallthrough]` | 3 | 1 |

### flow-013 安全な処理と分岐からの明示的return

主根拠：`[fn.flow-analysis]`。

補助根拠：`[if.elif]`。

**ソース：**

```suz
fn choose(value: i32) -> u8 {
    if value < 0 {
        return 1
    } elif value == 0 {
        return 2
    } else {
        return 3
    }
}

fn main() -> u8 {
    return choose(0)
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 2 | 0 |

### flow-014 while trueの内部から値を返す

主根拠：`[fn.flow-analysis]`。

補助根拠：`[fn.value-return-required]`。

**ソース：**

```suz
fn main() -> u8 {
    while true {
        return 3
    }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 3 | 0 |

条件はtrueリテラルで、ループを抜けるbreakはない。return式もあるため、関数末尾に別のreturnは必要ない。

### flow-015 trueの定数を条件にしたwhileは末尾へ進みうると判定する

主根拠：`[fn.flow-analysis]`。

補助根拠：`[fn.fallthrough]`。

**ソース：**

```suz
const ALWAYS = true

fn main() -> u8 {
    while ALWAYS {
        return 3
    }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

条件に書かれているのは定数名であり、trueリテラルではない。関数末尾へ進みうるという構文上の判定になる。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[fn.fallthrough]` | 7 | 1 |

### flow-016 反復があるforでも関数末尾への経路を残す

主根拠：`[fn.flow-analysis]`。

補助根拠：`[fn.fallthrough]`。

**ソース：**

```suz
fn main() -> u8 {
    for i in 0..<1 {
        return 3
    }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

範囲が0..<1であっても、forは後続へ進みうるものとして判定する。ループ内のreturnだけでは関数末尾への到達を防げない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[fn.fallthrough]` | 5 | 1 |

### flow-017 内側のbreakは外側のloopを抜けない

主根拠：`[fn.flow-analysis]`。

補助根拠：`[loop.break]`。

**ソース：**

```suz
fn main() -> u8 {
    loop {
        loop {
            break
        }
        return 4
    }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 4 | 0 |

breakが抜けるのは内側のloopだけ。外側のloopには、それを抜けるbreakがない。

### flow-018 実行されない分岐のbreakも到達判定で数える

主根拠：`[fn.flow-analysis]`。

補助根拠：`[fn.fallthrough]`。

**ソース：**

```suz
fn main() -> u8 {
    loop {
        if false {
            break
        }
        return 3
    }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

条件がfalseでも、外側のloopを抜けるbreakが構文上存在する。ループ後にreturnがないためエラーになる。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[fn.fallthrough]` | 8 | 1 |

### flow-019 matchの対象を1回評価し、一致した腕だけ実行する

主根拠：`[match.target]`。

**ソース：**

```suz
fn subject() -> i32 {
    log info `subject`
    return 2
}

fn main() -> u8 {
    match subject() {
        1 => { log info `first` }
        2 => { log info `second` }
        _ => { log info `other` }
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

subjectを1回呼び、値2に対応する腕だけを実行する。

**標準エラー：**

```text
info flow-019.suz:2: subject
info flow-019.suz:9: second
```

### flow-064 charのmatchと比較の連鎖を使う

主根拠：`[match.target]`。

**ソース：**

```suz
fn main() -> u8 {
    fix ch = '5'
    fix kind = matchexpr ch {
        ' ' => 1
        '5' => 2
        _ => 0
    }
    log info `${kind} ${'0' <= ch <= '9'}`
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info flow-064.suz:8: 2 true
```

### flow-020 整数matchの定数パターンとワイルドカード

主根拠：`[match.pattern]`。

補助根拠：`[match.exhaustive]`。

**ソース：**

```suz
const SELECTED: i32 = 2

fn main() -> u8 {
    fix input: i32 = 2
    match input {
        SELECTED => { return 3 }
        _ => { return 4 }
    }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 3 | 0 |

### flow-021 boolでワイルドカードを使う

主根拠：`[match.pattern]`。

**ソース：**

```suz
fn main() -> u8 {
    match true {
        true => {}
        false => {}
        _ => {}
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

全値を列挙していてもboolの_は禁止。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[match.pattern]` | 5 | 9 |

### flow-022 enumでワイルドカードを使う

主根拠：`[match.pattern]`。

**ソース：**

```suz
enum Wave: u8 { Sine, Saw }

fn main() -> u8 {
    match Wave.Sine {
        Wave.Sine => {}
        Wave.Saw => {}
        _ => {}
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

enumの_は禁止。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[match.pattern]` | 7 | 9 |

### flow-023 評価後のパターンの重複

主根拠：`[match.pattern]`。

**ソース：**

```suz
fn main() -> u8 {
    match 2 {
        1 + 1 => {}
        2 => {}
        _ => {}
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

1 + 1と2は同じ値。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[match.pattern]` | 4 | 9 |

### flow-024 整数matchの途中にワイルドカードを置く

主根拠：`[match.pattern]`。

補助根拠：`[match.exhaustive]`。

**ソース：**

```suz
fn main() -> u8 {
    match 1 {
        _ => {}
        1 => {}
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

整数の_は最後の腕に置く必要がある。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[match.pattern]` | 3 | 9 |

### flow-025 fixの変数をmatchのパターンにする

主根拠：`[match.pattern]`。

補助根拠：`[const.expression]`。

**ソース：**

```suz
fn main() -> u8 {
    fix selected: i32 = 1
    fix input: i32 = 1
    match input {
        selected => {}
        _ => {}
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

fixの変数は、数値リテラルで初期化しても定数式の構成要素にはならない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[const.expression]` | 5 | 9 |

### flow-026 boolの全値を網羅するmatchexpr

主根拠：`[match.exhaustive]`。

補助根拠：`[match.value]`。

**ソース：**

```suz
fn main() -> u8 {
    fix flag = true
    return matchexpr flag {
        true => 3
        false => 4
    }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 3 | 0 |

### flow-027 enumの全列挙子を網羅するmatchexpr

主根拠：`[match.exhaustive]`。

補助根拠：`[match.value]`。

**ソース：**

```suz
enum Wave: u8 { Sine, Saw }

fn main() -> u8 {
    fix wave = Wave.Saw
    return matchexpr wave {
        Wave.Sine => 1
        Wave.Saw => 2
    }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 2 | 0 |

### flow-028 boolの網羅漏れ

主根拠：`[match.exhaustive]`。

**ソース：**

```suz
fn main() -> u8 {
    match true {
        true => {}
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

falseの腕が必要。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[match.exhaustive]` | 2 | 5 |

### flow-029 整数matchのワイルドカード漏れ

主根拠：`[match.exhaustive]`。

**ソース：**

```suz
fn main() -> u8 {
    match 0 {
        0 => {}
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

整数matchには最後の_が必要。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[match.exhaustive]` | 2 | 5 |

### flow-030 enumの列挙子を網羅していないmatch

主根拠：`[match.exhaustive]`。

**ソース：**

```suz
enum Wave: u8 { Sine, Saw }

fn main() -> u8 {
    match Wave.Sine {
        Wave.Sine => {}
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

対象がWave.Sineと書かれていても、型の全列挙子を網羅する必要がある。Wave.Sawの腕がない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[match.exhaustive]` | 4 | 5 |

### flow-031 最後のmatchの腕と閉じ括弧を同じ行に書く

主根拠：`[match.separator]`。

補助根拠：`[src.statement-end]`、`[match.arm]`。

**ソース：**

```suz
fn main() -> u8 {
    match false {
        true => { return 1 }
        false => { return 2 } }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 2 | 0 |

腕同士は改行で区切るが、最後の腕のブロックとmatchを閉じる波括弧の間は改行を省略できる。

### flow-032 matchの腕を同じ行に並べる

主根拠：`[match.separator]`。

**ソース：**

```suz
fn main() -> u8 {
    match true {
        true => {} false => {}
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

ブロックを閉じても次の腕との間には改行が必要。最後の腕とmatchを閉じる波括弧の間だけ省略できる。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[match.separator]` | 3 | 20 |

### flow-033 文のmatchとifは分岐の結果を捨てる

主根拠：`[branch.statement]`。

補助根拠：`[match.exhaustive]`。

**ソース：**

```suz
fn main() -> u8 {
    if true {
        1
    } else {
        false
    }
    match false {
        true => 1
        false => false
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-034 expr末尾に値がない

主根拠：`[block.value]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = expr {
        log info `message`
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

末尾はログ文であり、値を持つ式ではない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[block.value]` | 4 | 5 |

### flow-035 exprの末尾をvoidの式にする

主根拠：`[block.value]`。

補助根拠：`[branch.expression]`。

**ソース：**

```suz
fn action() -> void {}

fn main() -> u8 {
    expr {
        action()
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

結果を変数に保存せず捨てる場合でも、expr自体はvoid以外の値を作る必要がある。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[block.value]` | 5 | 9 |

### flow-036 elseを次の行に置く

主根拠：`[expr.else-line]`。

**ソース：**

```suz
fn main() -> u8 {
    if true {
    }
    else {
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

elseは直前の}と同じ行に置く。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.else-line]` | 4 | 5 |

### flow-037 条件を順に評価し、成立後のelifを評価しない

主根拠：`[if.condition]`。

補助根拠：`[if.elif]`。

**ソース：**

```suz
fn check(id: u8, result: bool) -> bool {
    log info `${id}`
    return result
}

fn main() -> u8 {
    if check(1, false) {
        return 1
    } elif check(2, true) {
        return 2
    } elif check(3, true) {
        return 3
    } else {
        return 4
    }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 2 | 0 |

1番目の条件はfalse、2番目はtrue。3番目の条件を評価せず、2番目の分岐から戻る。

**標準エラー：**

```text
info flow-037.suz:2: 1
info flow-037.suz:2: 2
```

### flow-038 ifの条件に整数を使う

主根拠：`[if.condition]`。

補助根拠：`[type.bool]`。

**ソース：**

```suz
fn main() -> u8 {
    if 1 {
        return 1
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

整数の1はboolとして扱わない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[if.condition]` | 2 | 8 |

### flow-039 評価されないelifの条件にもboolを要求する

主根拠：`[if.condition]`。

**ソース：**

```suz
fn main() -> u8 {
    if true {
        return 0
    } elif 1 {
        return 1
    } else {
        return 2
    }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

先頭の条件がtrueでも、elifの条件の型は検査する。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[if.condition]` | 4 | 12 |

### flow-040 whileのcontinueで更新後の条件を再評価する

主根拠：`[while.loop]`。

countを0から1、1から2へ更新し、各反復のcontinueで9の代入を飛ばす。条件を再評価して2回で終了し、2を返す。continueをbreakとして扱うと1を返すため、区別できる。

**ソース：**

```suz
fn main() -> u8 {
    var count: u8 = 0
    while count < 2 {
        count += 1
        continue
        count = 9
    }
    return count
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 2 | 0 |

### flow-041 whileの条件に整数を使う

主根拠：`[while.loop]`。

補助根拠：`[type.bool]`。

**ソース：**

```suz
fn main() -> u8 {
    while 0 {}
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

整数の0はboolとして扱わない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[while.loop]` | 2 | 11 |

### flow-042 loop内のbreakとcontinue

主根拠：`[loop.unconditional]`。

補助根拠：`[loop.break]`、`[loop.continue]`。

**ソース：**

```suz
fn main() -> u8 {
    var count: u8 = 0
    loop {
        count += 1
        if count == 2 {
            continue
        }
        if count == 3 {
            break
        }
    }
    return count
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 3 | 0 |

### flow-043 forとrev専用の範囲記法を変数に保存する

主根拠：`[for.range]`。

**ソース：**

```suz
fn main() -> u8 {
    fix range = 0..<3
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

..<の範囲記法はfor専用であり、値を作る式として使えない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[for.range]` | 2 | 18 |

### flow-060 両端を左から評価し、範囲を逆順に繰り返す

主根拠：`[rev.range]`。

**ソース：**

```suz
fn low() -> u8 { log info `low`
return 1 }
fn high() -> u8 { log info `high`
return 3 }
fn main() -> u8 {
    rev i in low()..<high() { log info `${i}` }
    rev i in 1..=3 { log info `${i}` }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info flow-060.suz:1: low
info flow-060.suz:3: high
info flow-060.suz:6: 2
info flow-060.suz:6: 1
info flow-060.suz:7: 3
info flow-060.suz:7: 2
info flow-060.suz:7: 1
```

### flow-061 空の範囲では減算せず、両端を含む同値の範囲は1回繰り返す

主根拠：`[rev.bounds]`。

**ソース：**

```suz
fn main() -> u8 {
    fix low: i8 = -128
    rev i in low..<low { return 1 }
    rev i in 1..<0 { return 2 }
    rev i in 1..=0 { return 3 }
    var count: u8 = 0
    rev i in low..=low { count += 1 }
    return count
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 1 | 0 |

### flow-062 型の全範囲を逆順に進み、最小値でcontinueする

主根拠：`[rev.bounds]`。

**ソース：**

```suz
fn main() -> u8 {
    fix low: i8 = -128
    fix high: i8 = 127
    var count: u16 = 0
    var last: i8 = 0
    rev i in low..=high {
        count += 1
        last = i
        continue
    }
    log info `${count} ${last}`
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info flow-062.suz:11: 256 -128
```

### flow-065 文字範囲でサロゲートを飛ばし、両端でcontinueする

主根拠：`[char.range]`。

**ソース：**

```suz
fn main() -> u8 {
    for ch in '\u{D7FF}'..='\u{E000}' { log info `${ch as u32}` }
    rev ch in '\u{D7FF}'..='\u{E000}' { log info `${ch as u32}` }
    for ch in '\u{10FFFF}'..='\u{10FFFF}' { continue }
    rev ch in '\0'..='\0' { continue }
    rev ch in '\0'..<'\0' { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info flow-065.suz:2: 55295
info flow-065.suz:2: 57344
info flow-065.suz:3: 57344
info flow-065.suz:3: 55295
```

### flow-044 範囲演算子の前後の空白

主根拠：`[for.range-spacing]`。

**ソース：**

```suz
fn main() -> u8 {
    for i in 0 ..< 3 {
        log info `${i}`
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

..<は両端の式の境界に隣接する必要がある。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[for.range-spacing]` | 2 | 16 |

### flow-045 範囲演算子の直後にコメントと改行を挟む

主根拠：`[for.range-spacing]`。

補助根拠：`[src.line-continuation]`。

**ソース：**

```suz
fn main() -> u8 {
    for i in 0..<// 終了値は次の行
        3 {}
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

範囲演算子には両端への隣接が必要。ほかの二項演算子のように直後の改行で継続できない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[for.range-spacing]` | 2 | 15 |

### flow-046 forの両端に異なる整数型を使う

主根拠：`[for.bounds]`。

補助根拠：`[type.numeric-conversion]`。

**ソース：**

```suz
fn main() -> u8 {
    fix start: u8 = 0
    fix end: u16 = 3
    for i in start..<end {}
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

開始値のu8と終了値のu16は一致しない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.expected]` | 4 | 22 |

### flow-047 変数を使う範囲と最大値を含む範囲

主根拠：`[for.bounds]`。

補助根拠：`[type.context]`、`[for.variable]`。

**ソース：**

```suz
fn main() -> u8 {
    fix end: u8 = 3
    fix top: u8 = 255
    var total: u8 = 0
    for i in 0..<end {
        total += i
    }
    for last in top..=top {
        total += 1
    }
    return total
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 4 | 0 |

iとlastはu8。255の反復後に増分によるオーバーフローを起こさない。

### flow-048 空の範囲

主根拠：`[for.bounds]`。

**ソース：**

```suz
fn main() -> u8 {
    for i in 3..<3 {
        return 1
    }
    for j in 4..=3 {
        return 2
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-049 範囲の両端を左から1回だけ評価

主根拠：`[for.bounds]`。

補助根拠：`[expr.eval-order]`。

**ソース：**

```suz
fn lower() -> u8 {
    log info `lower`
    return 0
}

fn upper() -> u8 {
    log info `upper`
    return 2
}

fn main() -> u8 {
    for i in lower()..<upper() {
        log info `i=${i}`
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info flow-049.suz:2: lower
info flow-049.suz:7: upper
info flow-049.suz:13: i=0
info flow-049.suz:13: i=1
```

### flow-050 変更可能な終了値をfixにコピーして範囲に使う

主根拠：`[for.bound-reference]`。

補助根拠：`[for.bounds]`。

**ソース：**

```suz
fn main() -> u8 {
    var end: u8 = 3
    fix limit = end
    var count: u8 = 0
    for i in 0..<limit {
        end = 0
        count += 1
    }
    return count
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 3 | 0 |

endの現在値をfixのlimitにコピーする。範囲はlimitを使うため、本体でendを変更しても3回反復する。

### flow-051 forの終了値にvarを直接使う

主根拠：`[for.bound-reference]`。

**ソース：**

```suz
fn main() -> u8 {
    var end: u8 = 3
    for i in 0..<end {
        end = 0
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

endを範囲式から直接参照している。現在値を使う場合は先にfixへコピーする。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[for.bound-reference]` | 3 | 18 |

### flow-052 範囲の開始値の関数引数にvarを使う

主根拠：`[for.bound-reference]`。

**ソース：**

```suz
fn identity(value: u8) -> u8 {
    return value
}

fn main() -> u8 {
    var start: u8 = 0
    for i in identity(start)..<3 {
        start = 1
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

開始値の式にある実引数も参照の検査対象になる。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[for.bound-reference]` | 7 | 23 |

### flow-053 範囲の終了値にvarのフィールドを使う

主根拠：`[for.bound-reference]`。

**ソース：**

```suz
struct Limits { end: u8 }

fn main() -> u8 {
    var limits = Limits { end: 3 }
    for i in 0..<limits.end {
        limits.end = 0
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

limitsのフィールドへの参照もvarからの参照として扱う。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[for.bound-reference]` | 5 | 18 |

### flow-067 textの反復対象でvarを参照する

主根拠：`[for.bound-reference]`。

**ソース：**

```suz
fn main() -> u8 {
    var message = `abc`
    for ch in message {}
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
| 通常 | `[for.bound-reference]` | 3 | 15 |

### flow-054 ループ変数への代入

主根拠：`[for.variable]`。

**ソース：**

```suz
fn main() -> u8 {
    for i in 0..<3 {
        i = 2
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

ループ変数は変更不可。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[for.variable]` | 3 | 9 |

### flow-055 forの外からループ変数を参照する

主根拠：`[for.variable]`。

**ソース：**

```suz
fn main() -> u8 {
    fix end: u8 = 3
    for i in 0..<end {}
    return i
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

iを参照できるのはループ本体の中だけ。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[decl.local-scope]` | 4 | 12 |

### flow-066 textをスカラー値単位で前後に反復する

主根拠：`[char.for]`。

**ソース：**

```suz
fn read(value: symbol) -> symbol {
    log info `read`
    return value
}
fn main() -> u8 {
    var message = `aあ😀`
    fix target = message
    for ch in read(target) {
        log info `${ch}`
        message = `changed`
        continue
    }
    rev ch in read(target) {
        if ch == 'a' { break }
        log info `${ch}`
    }
    for ch in `` { return 1 }
    rev ch in `` { return 2 }
    log info `${message}`
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info flow-066.suz:2: read
info flow-066.suz:9: a
info flow-066.suz:9: あ
info flow-066.suz:9: 😀
info flow-066.suz:2: read
info flow-066.suz:15: 😀
info flow-066.suz:15: あ
info flow-066.suz:19: changed
```

### flow-056 forのcontinueは次の値へ進む

主根拠：`[loop.continue]`。

補助根拠：`[for.bounds]`。

**ソース：**

```suz
fn main() -> u8 {
    fix end: u8 = 3
    var total: u8 = 0
    for i in 1..=end {
        if i == 2 {
            continue
        }
        total += i
    }
    return total
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 4 | 0 |

iが2の反復では加算を飛ばす。1と3を加えた結果は4になる。

### flow-063 入れ子のrevからbreakし、外側のforを続ける

主根拠：`[loop.break]`。

**ソース：**

```suz
fn main() -> u8 {
    var count: u8 = 0
    for outer in 0..<2 {
        rev inner in 0..<4 {
            if inner == 1 { break }
            if inner == 2 { continue }
            count += 1
        }
    }
    return count
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 2 | 0 |

### flow-057 ループの外にbreakを書く

主根拠：`[loop.jump-context]`。

**ソース：**

```suz
fn main() -> u8 {
    break
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
| 通常 | `[loop.jump-context]` | 2 | 5 |

### flow-058 ループの外にcontinueを書く

主根拠：`[loop.jump-context]`。

**ソース：**

```suz
fn main() -> u8 {
    continue
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
| 通常 | `[loop.jump-context]` | 2 | 5 |

### flow-068 戻り値だけに汎用型を持つ関数を呼ぶ

主根拠：`[num.fn]`。

仮引数がなくても、戻り値に `int` を持つ関数は汎用関数である。受け取り先から具体型を決める。

**ソース：**

```suz
fn one() -> int {
    return 1
}
fn main() -> u8 {
    fix small: u8 = one()
    fix large: u64 = one()
    return ifexpr small == 1 && large == 1 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-069 引数・戻り値・ローカル変数を同じ具体型にする

主根拠：`[num.fn-same]`。

補助根拠：`[num.fn-body-type]`、`[num.fn-recursion]`。

`count` の引数・戻り値・`previous` は、最初の呼び出しでは `u8`、次の呼び出しでは `u16` になる。再帰呼び出しでも、それぞれ同じ具体型を使う。

**ソース：**

```suz
fn count(n: int) -> int {
    if n == 0 { return 0 }
    fix previous: int = count(n - 1)
    return previous + 1
}
fn main() -> u8 {
    fix small: u8 = 3
    fix large: u16 = 300
    fix a = count(small)
    fix b = count(large)
    return ifexpr a == 3 && b == 300 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-070 独立したintとfloat、候補のない呼び出し

主根拠：`[num.fn-resolve]`。

`first` の `int` は `u8`、`float` は `f32` になる。`zero()` は受け取り先の要求型がなければ `i64`、`narrow` の初期値では `u8` になる。`half(4.0)` では受け取り先の `f32` を使う。

**ソース：**

```suz
fn first(value: int, factor: float) -> int {
    return value
}
fn zero() -> int {
    return 0
}
fn half(value: float) -> float {
    return value * 0.5
}
fn main() -> u8 {
    fix value: u8 = 12
    fix factor: f32 = 0.5
    fix picked = first(value, factor)
    fix plain = zero()
    fix narrow: u8 = zero()
    fix scaled: f32 = half(4.0)
    return ifexpr picked == 12 && plain == 0 && narrow == 0 && scaled == 2.0 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-071 intの結果を浮動小数点型で受け取る

主根拠：`[num.fn-convert]`。

補助根拠：`[num.fn-resolve]`。

`f64` は `int` の型候補に入らない。`identity(input)` は `u8` を返し、その結果を損失なく `f64` に広げる。算術式を渡した呼び出しでは、要求型なしで決まる `u8` を使う。

**ソース：**

```suz
fn identity(value: int) -> int {
    return value
}
fn main() -> u8 {
    fix input: u8 = 3
    fix result: f64 = identity(input)
    fix sum: u8 = identity(input + input)
    return ifexpr result == 3.0 && sum == 6 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-072 floatを返す内側の呼び出しをintの引数に渡す

主根拠：`[num.fn-convert]`。

補助根拠：`[num.fn-nested]`。

`half` の戻り値は `f32` であり、`int` の引数には渡せない。種類が異なる呼び出し間では具体型をそろえない。

**ソース：**

```suz
fn half(value: float) -> float {
    return value * 0.5
}
fn identity(value: int) -> int {
    return value
}
fn main() -> u8 {
    fix input: f32 = 4.0
    fix result = identity(half(input))
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
| 通常 | `[num.fn-convert]` | 9 | 27 |

### flow-073 入れ子の呼び出し全体で具体型を決める

主根拠：`[num.fn-nested]`。

`input` の `u8` と受け取り先の `u16` をそろえ、`square` と `clamp` の `int` をともに `u16` にする。二乗はオーバーフローせず、40000を返す。

**ソース：**

```suz
fn square(value: int) -> int {
    return value * value
}
fn clamp(value: int, lo: int, hi: int) -> int {
    return ifexpr value < lo { lo } elif value > hi { hi } else { value }
}
fn main() -> u8 {
    fix input: u8 = 200
    fix result: u16 = clamp((square(input)), 0, 50_000)
    return ifexpr result == 40_000 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-074 汎用検査の診断にはintを使う

主根拠：`[num.fn-check]`。

呼び出しで型を決める前に、戻り値のboolがintと一致しないことを検出する。診断の型名はintとなる。

**ソース：**

```suz
fn identity(value: int) -> int {
    return true
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
| 通常 | `[type.expected]` | 2 | 12 |

### flow-075 型に依存する判定を保留して外側の演算を検査する

主根拠：`[num.fn-check-judge]`。

300が収まるかは具体型に依存するため保留する。その式とboolの加算はどの具体型でもできないため、本体で診断する。

**ソース：**

```suz
fn add(value: int) -> int {
    fix result = (value +% 300) + true
    return result
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
| 通常 | `[expr.operator-types]` | 2 | 35 |

### flow-076 呼び出しで決まった型が本体の戻り値型に収まらない

主根拠：`[num.fn-body]`。

`small` の `int` は `u16` になる。本体の `return value` は `u8` へ暗黙に縮小できないため、呼び出し位置に診断し、本体の違反位置も示す。

**ソース：**

```suz
fn small(value: int) -> u8 {
    return value
}
fn main() -> u8 {
    fix value: u16 = 300
    return small(value)
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[num.fn-body]` | 6 | 12 |

### flow-077 汎用検査を打ち切るまでに保留した判定を呼び出しで行う

主根拠：`[num.fn-body]`。

補助根拠：`[num.fn-check]`。

300がintに収まるかを保留した後で、boolへの整数の代入を検出する。呼び出しでは先に保留した判定を行い、u8に300が収まらないことを報告する。関連診断は7行18列の `[lit.range]` となる。

**ソース：**

```suz
fn main() -> u8 {
    fix a: u8 = 1
    fix k = f(a)
    return 0
}
fn f(n: int) -> int {
    fix y = n +% 300
    fix z: bool = 1
    return n
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[num.fn-body]` | 3 | 13 |

### flow-078 汎用検査を打ち切った後の判定は呼び出しで行わない

主根拠：`[num.fn-body]`。

補助根拠：`[num.fn-check]`。

boolへの整数の代入で汎用検査を打ち切るため、その後の300については判定を保留していない。呼び出し位置ではエラーを出さず、本体の違反を宣言順に報告する。

**ソース：**

```suz
fn main() -> u8 {
    fix a: u8 = 1
    fix k = f(a)
    return 0
}
fn f(n: int) -> int {
    fix z: bool = 1
    fix y = n +% 300
    return n
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.expected]` | 7 | 19 |

### flow-079 未使用の汎用関数から未宣言の名前を参照する

主根拠：`[num.fn-unused]`。

`unused` を呼び出さなくても、`missing` が未宣言であることは検出する。

**ソース：**

```suz
fn unused(value: int) -> int {
    return missing
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
| 通常 | `[decl.local-scope]` | 2 | 12 |

### flow-080 未使用関数の型に依存しない誤りを検出する

主根拠：`[num.fn-unused]`。

`narrowed` の初期化は具体型によって成否が変わるが、整数リテラルを `text` にする初期化は具体型によらず誤りである。前の文の型検査に失敗する組み合わせがあっても、後の誤りを見逃さない。

**ソース：**

```suz
fn unused(value: int) -> u8 {
    fix narrowed: u8 = value
    fix message: text = 5
    return narrowed
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
| 通常 | `[type.expected]` | 3 | 25 |

### flow-081 未使用関数の整数演算にもallowを適用する

主根拠：`[num.fn-unused]`。
補助根拠：`[allow.unused]`。

関数を呼び出さなくても整数演算の警告対象を集計する。警告設定を省略した場合も、対象があるためallowを受理する。強制設定のエラーは抑制しない。

**ソース：**

```suz
fn next(value: int) -> int {
    return value + 1 allow [diagnostic.arithmetic: bounded by the caller]
}
fn main() -> u8 {
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |
| 警告 | 正常終了 | 0 | 0 |
| 強制 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 強制 | `[diagnostic.arithmetic]` | 2 | 18 |

### flow-082 未使用関数でもすべての具体型で一定の比較を警告する

主根拠：`[num.fn-warning]`。

`value >= 0` は符号付き整数型では一定にならず、警告しない。`small >= 0` はどの具体型でも常に真になるため、関数を呼ばなくても1回警告する。

**ソース：**

```suz
fn nonnegative(value: int) -> bool {
    return value >= 0
}
fn always(value: int) -> bool {
    fix small: u8 = 5
    return small >= 0
}
fn main() -> u8 {
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
| 通常 | `[diagnostic.constant-comparison]` | 6 | 18 |

### flow-083 宣言していないfloatを変換先に使う

主根拠：`[num.fn-undeclared]`。

関数の引数型と戻り値型に現れる汎用型は `int` だけである。`float` は関数本体の変換先にも使えない。

**ソース：**

```suz
fn convert(value: int) -> int {
    fix converted = value as float
    return value
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
| 通常 | `[num.fn-undeclared]` | 2 | 27 |

### flow-084 本体のint注釈と注釈なしの型推論を区別する

主根拠：`[num.fn-body-type]`。

`typed` は呼び出しごとに `u8`・`u64` となり、255に1を加えるラップ演算の結果は0・256となる。注釈のない `inferred` はどちらも `i64` となり、結果は256となる。

**ソース：**

```suz
fn values(input: int) -> int {
    fix typed: int = 255
    fix inferred = 255
    if inferred +% 1 != 256 { return 99 }
    return typed +% 1
}
fn main() -> u8 {
    fix small: u8 = 1
    fix large: u64 = 1
    return ifexpr values(small) == 0 && values(large) == 256 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-085 安全な具体型で呼び出してもキャストのorを省略できない

主根拠：`[num.fn-cast]`。

この呼び出しでは `int` が `u8` になるが、他の具体型では変換が失敗しうる。関数本体の `as u8` には `or` が必要である。

**ソース：**

```suz
fn narrow(value: int) -> u8 {
    return value as u8
}
fn main() -> u8 {
    fix value: u8 = 1
    return narrow(value)
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[num.fn-cast]` | 2 | 18 |

### flow-086 具体型を経由した定数のキャストを受理する

主根拠：`[num.fn-cast]`。

中間の `u16` への変換後も、内側の `as% int` による値の違いを判定する。1はすべての具体型で最終的に `u8` に収まる。

**ソース：**

```suz
fn narrow(input: int) -> u8 {
    return ((1 as% int) as u16 or 0) as u8
}
fn main() -> u8 {
    fix input: u8 = 0
    return ifexpr narrow(input) == 1 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-087 具体型を経由した定数のキャストにもorを要求する

主根拠：`[num.fn-cast]`。

中間の `u16` への変換後も、内側の `as% int` による値の違いを判定する。300は `int` が `u16` などの場合に `u8` に収まらない。呼び出しで `u8` を選んでも、末尾の `as u8` に `or` が必要である。

**ソース：**

```suz
fn narrow(input: int) -> u8 {
    return ((300 as% int) as u16 or 0) as u8
}
fn main() -> u8 {
    fix input: u8 = 0
    return ifexpr narrow(input) == 1 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[num.fn-cast]` | 2 | 40 |

### flow-088 汎用関数同士を相互再帰で呼ぶ

主根拠：`[num.fn-recursion]`。

`even` と `odd` を行き来しても、各呼び出しで決まった `u8`・`u16` を引き継ぐ。

**ソース：**

```suz
fn even(value: int) -> bool {
    if value == 0 { return true }
    return odd(value - 1)
}
fn odd(value: int) -> bool {
    if value == 0 { return false }
    return even(value - 1)
}
fn main() -> u8 {
    fix small: u8 = 4
    fix large: u16 = 301
    return ifexpr even(small) && odd(large) { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-089 mainの戻り値をintにする

主根拠：`[num.fn-main]`。

`main` の戻り値は `u8` であり、汎用型の `int` にはできない。

**ソース：**

```suz
fn main() -> int {
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
| 通常 | `[prog.main]` | 1 | 14 |
