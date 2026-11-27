# 関数と制御

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
    value.point#x = 9
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
fix ALWAYS = true

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
    log info "subject"
    return 2
}

fn main() -> u8 {
    match subject() {
        1 => { log info "first" }
        2 => { log info "second" }
        _ => { log info "other" }
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
    fix digit = '0' <= ch <= '9'
    log info "${kind} ${digit}"
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info flow-064.suz:9: 2 true
```

### flow-020 整数matchの定数パターンとワイルドカード

主根拠：`[match.pattern]`。

補助根拠：`[match.exhaustive]`。

**ソース：**

```suz
fix SELECTED: i32 = 2

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
        log info "message"
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
    log info "${id}"
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
    fix items = 0..<3
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
fn low() -> u8 { log info "low"
return 1 }
fn high() -> u8 { log info "high"
return 3 }
fn main() -> u8 {
    rev i in low()..<high() { log info "${i}" }
    rev i in 1..=3 { log info "${i}" }
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
    log info "${count} ${last}"
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
    for ch in '\u{D7FF}'..='\u{E000}' {
        fix code = ch as u32
        log info "${code}"
    }
    rev ch in '\u{D7FF}'..='\u{E000}' {
        fix code = ch as u32
        log info "${code}"
    }
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
info flow-065.suz:4: 55295
info flow-065.suz:4: 57344
info flow-065.suz:8: 57344
info flow-065.suz:8: 55295
```

### flow-044 範囲演算子の前後の空白

主根拠：`[for.range-spacing]`。

**ソース：**

```suz
fn main() -> u8 {
    for i in 0 ..< 3 {
        log info "${i}"
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
    log info "lower"
    return 0
}

fn upper() -> u8 {
    log info "upper"
    return 2
}

fn main() -> u8 {
    for i in lower()..<upper() {
        log info "i=${i}"
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
        limits#end = 0
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
    var message = "abc"
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
fn read(value: text) -> text {
    log info "read"
    return value
}
fn main() -> u8 {
    var message = "aあ😀"
    fix target = message
    for ch in read(target) {
        log info "${ch}"
        message = "changed"
        continue
    }
    rev ch in read(target) {
        if ch == 'a' { break }
        log info "${ch}"
    }
    for ch in "" { return 1 }
    rev ch in "" { return 2 }
    log info "${message}"
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

### flow-092 値を捕捉したクロージャを渡す

主根拠：`[closure.expression]`。

**ソース：**

```suz
fnshape Calc(i32) -> i32
fn apply(f: fn Calc, value: i32) -> i32 { return f(value) }
fn main() -> u8 {
    fix scale: i32 = 2
    fix result = apply(fn [scale](n: i32) -> i32 { return n * scale }, 3)
    if result != 6 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-093 クロージャを変数に保存しない

主根拠：`[closure.place]`。

**ソース：**

```suz
fn main() -> u8 {
    fix f = fn []() -> i32 { return 1 }
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
| 通常 | `[closure.place]` | 2 | 13 |

### flow-094 クロージャを丸括弧で包まない

主根拠：`[closure.place]`。

**ソース：**

```suz
fnshape Once() -> i32
fn run(f: fn Once) -> i32 { return f() }
fn main() -> u8 {
    fix n = run((fn []() -> i32 { return 1 }))
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
| 通常 | `[closure.place]` | 4 | 18 |

### flow-095 外側の変数を暗黙に捕捉しない

主根拠：`[closure.scope]`。

**ソース：**

```suz
fnshape Once() -> i32
fn run(f: fn Once) -> i32 { return f() }
fn main() -> u8 {
    fix scale: i32 = 2
    fix n = run(fn []() -> i32 { return scale })
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
| 通常 | `[closure.scope]` | 5 | 41 |

### flow-096 fixを参照キャプチャーしない

主根拠：`[closure.capture-list]`。

**ソース：**

```suz
fnshape Once() -> i32
fn run(f: fn Once) -> i32 { return f() }
fn main() -> u8 {
    fix scale: i32 = 2
    fix n = run(fn [var scale]() -> i32 { return scale })
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
| 通常 | `[closure.capture-list]` | 5 | 21 |

### flow-097 同じ名前を2回捕捉しない

主根拠：`[closure.capture-list]`。

**ソース：**

```suz
fnshape Once() -> i32
fn run(f: fn Once) -> i32 { return f() }
fn main() -> u8 {
    fix scale: i32 = 2
    fix n = run(fn [scale, scale]() -> i32 { return scale })
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
| 通常 | `[closure.capture-list]` | 5 | 28 |

### flow-098 トップレベルの定数を捕捉一覧へ書かない

主根拠：`[closure.capture-list]`。

**ソース：**

```suz
fix SCALE: i32 = 2
fnshape Once() -> i32
fn run(f: fn Once) -> i32 { return f() }
fn main() -> u8 {
    fix n = run(fn [SCALE]() -> i32 { return SCALE })
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
| 通常 | `[closure.capture-list]` | 5 | 21 |

### flow-099 値キャプチャーは作成時の値を保つ

主根拠：`[closure.capture-value]`。

**ソース：**

```suz
fnshape Calc(i32) -> i32
fn apply(f: fn Calc, value: i32) -> i32 { return f(value) }
fn main() -> u8 {
    var scale: i32 = 2
    fix result = apply(fn [scale](n: i32) -> i32 { return n * scale }, expr { scale = 10
        3 })
    if result != 6 || scale != 10 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-100 値キャプチャーを書き換えない

主根拠：`[closure.capture-value]`。

**ソース：**

```suz
fnshape Once() -> i32
fn run(f: fn Once) -> i32 { return f() }
fn main() -> u8 {
    fix scale: i32 = 2
    fix n = run(fn [scale]() -> i32 { scale = 3
        return scale })
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
| 通常 | `[closure.capture-value]` | 5 | 39 |

### flow-101 参照キャプチャーから外側を更新する

主根拠：`[closure.capture-reference]`。

**ソース：**

```suz
fnshape Action() -> void
fn run(f: fn Action) -> void { f() }
fn main() -> u8 {
    var count: i32 = 0
    run(fn [var count]() -> void { count += 1 })
    if count != 1 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-102 クロージャ自身を匿名汎用にしない

主根拠：`[closure.generic]`。

**ソース：**

```suz
fnshape Calc(i32) -> i32
fn apply(f: fn Calc, value: i32) -> i32 { return f(value) }
fn main() -> u8 {
    fix result = apply(fn [](n: int) -> i32 { return 0 }, 3)
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
| 通常 | `[closure.generic]` | 4 | 24 |

### flow-103 名前から作った関数値を呼ぶ

主根拠：`[fn.value-from-name]`。

**ソース：**

```suz
fn twice(n: i32) -> i32 { return n * 2 }
fn main() -> u8 {
    fix other = fn twice
    if other(3) != 6 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-104 mainを関数値にしない

主根拠：`[fn.value-from-name]`。

**ソース：**

```suz
fn main() -> u8 {
    fix f = fn main
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
| 通常 | `[fn.value-from-name]` | 2 | 13 |

### flow-105 fnshape名だけでは型を書けない

主根拠：`[fn.type-mark]`。

**ソース：**

```suz
fnshape Transform(i32) -> i32
fn twice(n: i32) -> i32 { return n * 2 }
fn main() -> u8 {
    fix f: Transform = fn twice
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
| 通常 | `[fn.type-mark]` | 4 | 12 |

### flow-106 fnの後ろにはfnshapeを指定する

主根拠：`[fn.type-mark]`。

**ソース：**

```suz
fn main() -> u8 {
    fix f: static fn i32 = fn main
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
| 通常 | `[fn.type-mark]` | 2 | 22 |

### flow-107 呼び出し専用の関数を変数へ保存しない

主根拠：`[fn.type-call]`。

**ソース：**

```suz
fnshape Transform(i32) -> i32
fn twice(n: i32) -> i32 { return n * 2 }
fn main() -> u8 {
    fix f: fn Transform = fn twice
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
| 通常 | `[fn.type-call]` | 4 | 12 |

### flow-108 呼び出し専用の関数を戻り値型にしない

主根拠：`[fn.type-call]`。

**ソース：**

```suz
fnshape Transform(i32) -> i32
fn twice(n: i32) -> i32 { return n * 2 }
fn get() -> fn Transform { return fn twice }
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
| 通常 | `[fn.type-call]` | 3 | 13 |

### flow-109 呼び出し専用の仮引数を返さない

主根拠：`[fn.call-only]`。

**ソース：**

```suz
fnshape Transform(i32) -> i32
fn get(f: fn Transform) -> static fn Transform { return f }
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
| 通常 | `[fn.call-only]` | 2 | 57 |

### flow-110 呼び出し専用の仮引数を括弧で転送しない

主根拠：`[fn.call-only]`。

**ソース：**

```suz
fnshape Calc(i32) -> i32
fn apply(f: fn Calc, value: i32) -> i32 { return f(value) }
fn forward(f: fn Calc) -> i32 { return apply((f), 1) }
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
| 通常 | `[fn.call-only]` | 3 | 47 |

### flow-111 保存可能な関数値を呼び出し専用の仮引数へ渡す

主根拠：`[fn.type-convert]`。

**ソース：**

```suz
fnshape Calc(i32) -> i32
fn apply(f: fn Calc, value: i32) -> i32 { return f(value) }
fn twice(n: i32) -> i32 { return n * 2 }
fn main() -> u8 {
    fix f: static fn Calc = fn twice
    if apply(f, 3) != 6 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-112 fnを呼び出し式に付けない

主根拠：`[fn.value-mark]`。

**ソース：**

```suz
fnshape Transform(i32) -> i32
fn twice(n: i32) -> i32 { return n * 2 }
fn pick(i: i32) -> static fn Transform { return fn twice }
fn main() -> u8 {
    fix f = fn pick(0)
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
| 通常 | `[fn.value-mark]` | 5 | 20 |

### flow-113 返された関数値をその場で呼ぶ

主根拠：`[fn.value-call]`。

**ソース：**

```suz
fnshape Transform(i32) -> i32
fn twice(n: i32) -> i32 { return n * 2 }
fn pick(i: i32) -> static fn Transform { return fn twice }
fn main() -> u8 {
    if pick(0)(3) != 6 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-114 引数で変数を更新しても呼ぶ関数は変わらない

主根拠：`[fn.value-evaluation]`。

**ソース：**

```suz
fnshape Transform(i32) -> i32
fn twice(n: i32) -> i32 { log info "twice"
    return n * 2 }
fn triple(n: i32) -> i32 { log info "triple"
    return n * 3 }
fn main() -> u8 {
    var f: static fn Transform = fn twice
    fix result = f(expr { log info "argument"
        f = fn triple
        3 })
    if result != 6 || f(3) != 9 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info flow-114.suz:8: argument
info flow-114.suz:2: twice
info flow-114.suz:4: triple
```

### flow-115 関数値を比較しない

主根拠：`[fn.value-compare]`。

**ソース：**

```suz
fn twice(n: i32) -> i32 { return n * 2 }
fn main() -> u8 {
    fix f = fn twice
    fix equal = f == f
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
| 通常 | `[fn.value-compare]` | 4 | 19 |

### flow-116 関数値をテンプレートに埋め込まない

主根拠：`[fn.value-compare]`。

**ソース：**

```suz
fn twice(n: i32) -> i32 { return n * 2 }
fn main() -> u8 {
    fix f = fn twice
    fix rendered: symbol = "${f}"
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
| 通常 | `[symbol.interpolation]` | 4 | 31 |

### flow-117 要求型から匿名汎用の関数値を具体化する

主根拠：`[fn.value-generic]`。

**ソース：**

```suz
fnshape Transform(i32) -> i32
fn identity(n: int) -> int { return n }
fn main() -> u8 {
    fix action: static fn Transform = fn identity
    if action(3) != 3 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-118 匿名汎用の関数値には要求型が必要

主根拠：`[fn.value-generic]`。

**ソース：**

```suz
fn identity(n: int) -> int { return n }
fn main() -> u8 {
    fix f = fn identity
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
| 通常 | `[fn.value-generic]` | 3 | 13 |

### flow-119 fnshapeの引数にvoidを使わない

主根拠：`[fnshape.decl]`。

**ソース：**

```suz
fnshape Wrong(void) -> i32
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
| 通常 | `[type.void]` | 1 | 9 |

### flow-120 fnshapeの引数にintを使わない

主根拠：`[fnshape.decl]`。

**ソース：**

```suz
fnshape Wrong(int) -> i32
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
| 通常 | `[num.places]` | 1 | 9 |

### flow-121 名前が違っても同じ形の関数型を渡せる

主根拠：`[fnshape.same]`。

**ソース：**

```suz
fnshape Transform(i32) -> i32
fnshape Other(i32) -> i32
fn apply(f: fn Other, n: i32) -> i32 { return f(n) }
fn twice(n: i32) -> i32 { return n * 2 }
fn main() -> u8 {
    fix f: static fn Transform = fn twice
    if apply(f, 3) != 6 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-122 fnshapeの再帰を拒否する

主根拠：`[fnshape.recursion]`。

**ソース：**

```suz
fnshape Recursive(static fn Recursive) -> void
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
| 通常 | `[fnshape.recursion]` | 1 | 9 |

### flow-123 関数の事前条件・事後条件と範囲推定の連携

主根拠：`[contract.decl]`。
補助根拠：`[contract.requires]`、`[contract.ensures]`、`[arith.width]`。

`@requires(1 <= n <= 100)` と `@ensures(0 <= return <= 200)` を宣言すると、本体内の `(n * 2) as u8` で `or` を省略でき、呼び出し元でも戻り値の範囲を引き継いで `u8` への `as` の `or` を省略できる。

**ソース：**

```suz
@requires(1 <= n <= 100)
@ensures(0 <= return <= 200)
fn scale(n: i32) -> i32 {
    return (n * 2) as u8
}

fn main() -> u8 {
    fix r = scale(50)
    fix out: u8 = r as u8
    if out == 100 {
        return 0
    }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-124 mainに契約を付けると拒否する

主根拠：`[contract.decl]`。

`main` 関数には `@requires`・`@ensures` を付けられない。

**ソース：**

```suz
@ensures(return == 0)
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
| 通常 | `[contract.decl]` | 1 | 1 |

### flow-125 契約と関数宣言の間の空行を拒否する

主根拠：`[contract.decl]`。

契約の行と対象関数の間に空行を置くとコンパイルエラーとなる。

**ソース：**

```suz
@requires(n > 0)

fn pos(n: i32) -> i32 {
    return n
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
| 通常 | `[contract.decl]` | 2 | 1 |

### flow-126 契約式に関数呼び出しを書くと拒否する

主根拠：`[contract.expr]`。

契約式には通常の関数・メソッド呼び出しを書けない。

**ソース：**

```suz
fn ok(n: i32) -> bool {
    return n > 0
}

@requires(ok(n))
fn use_n(n: i32) -> i32 {
    return n
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
| 通常 | `[contract.expr]` | 5 | 11 |

### flow-127 契約式で非零保証のない除算を拒否する

主根拠：`[contract.expr]`。

除数が `0` になり得る除算は契約式に書けない。

**ソース：**

```suz
@requires(total / n < 10)
fn ratio(total: i32, n: i32) -> i32 {
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
| 通常 | `[contract.expr]` | 1 | 17 |

### flow-128 契約式の&&左辺で非零を保証すれば右辺の除算を許可する

主根拠：`[contract.expr]`。
補助根拠：`[contract.requires]`、`[contract.proof]`。

`@requires(n != 0 && total / n < 10)` のように短絡評価の左辺で非零を保証すれば右辺の除算を許可し、呼び出し側で同じ条件をガードすれば受理する。

**ソース：**

```suz
@requires(n != 0 && total / n < 10)
fn ratio(total: i32, n: i32) -> i32 {
    return 1
}

fn check(total: i32, n: i32) -> u8 {
    if n != 0 && total / n < 10 {
        if ratio(total, n) == 1 {
            return 0
        }
    }
    return 1
}

fn main() -> u8 {
    return check(20, 4)
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-129 void関数の@ensuresでreturnを参照すると拒否する

主根拠：`[contract.expr]`。

`void` を返す関数の `@ensures` では `return` を参照できない。

**ソース：**

```suz
@ensures(return == 0)
fn noop() -> void {
    return
}

fn main() -> u8 {
    noop()
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
| 通常 | `[contract.expr]` | 1 | 10 |

### flow-130 ガードなしで動的な値を@requires付き関数へ渡すと拒否する

主根拠：`[contract.requires]`。

動的な値 `n` を `if` ガードなしで `@requires(1 <= n <= 100)` の関数へ渡すとコンパイルエラーとなる。

**ソース：**

```suz
@requires(1 <= n <= 100)
fn scale(n: i32) -> u8 {
    return (n * 2) as u8
}

fn run(n: i32) -> u8 {
    return scale(n)
}

fn main() -> u8 {
    return run(10)
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[contract.requires]` | 7 | 12 |

### flow-131 @requiresの配列境界条件で本体内の添字orを省略する

主根拠：`[contract.requires]`。
補助根拠：`[array.bounds-proof]`。

`@requires(0 <= idx < len buf)` や `@requires(len dst == len src)` により、関数本体内の配列読み書きで `or` を省略できる。

**ソース：**

```suz
@requires(0 <= idx < len buf)
fn read_at(buf: .[u8], idx: i32) -> u8 {
    return buf.[idx]
}

@requires(len dst == len src)
fn copy_all(dst: #[u8], src: .[u8]) -> void {
    for i in 0..<len src {
        dst#[i] = src.[i]
    }
}

fn main() -> u8 {
    fix src: .[u8] = .[10, 20, 30]
    fix dst: #[u8] = new #[0; 3]
    if len dst == len src {
        copy_all(dst, src)
    }
    fix idx: i32 = 2
    if 0 <= idx < len dst {
        if read_at(dst, idx) == 30 {
            return 0
        }
    }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-132 実引数の評価中に参照先が変わり得る場合は先のガードで@requiresを証明しない

主根拠：`[contract.requires]`。
補助根拠：`[contract.proof]`。

`if v.active` の後に `use_voice(v, disable(v))` を呼ぶと、第 2 引数 `disable(v)` の評価で共有参照先の条件が無効化されるため、コンパイルエラーとなる。

**ソース：**

```suz
struct Voice {
    active: bool,
}

fn disable(v: mut Voice) -> i32 {
    v#active = false
    return 0
}

@requires(v.active)
fn use_voice(v: ref Voice, tag: i32) -> u8 {
    return 0
}

fn main() -> u8 {
    fix v = new Voice { active: true }
    if v.active {
        return use_voice(v, disable(v))
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
| 通常 | `[contract.requires]` | 18 | 16 |

### flow-133 return前のifガードと代替値で@ensuresを満たす

主根拠：`[contract.ensures]`。

動的な計算結果を返す場合でも、`return` 前の `if` ガードと範囲内の代替値によって `@ensures` を静的に満たせる。

**ソース：**

```suz
@ensures(0 <= return <= 200)
fn clamp_even(x: i32) -> i32 {
    if 0 <= x <= 200 {
        return x
    }
    return 0
}

fn main() -> u8 {
    fix a = clamp_even(42)
    fix b = clamp_even(999)
    fix ua: u8 = a as u8
    fix ub: u8 = b as u8
    if ua == 42 && ub == 0 {
        return 0
    }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-134 @ensuresを証明できないreturnを拒否する

主根拠：`[contract.ensures]`。

`@ensures(0 <= return <= 200)` を満たすことを証明できない `return` はコンパイルエラーとなる。

**ソース：**

```suz
@ensures(0 <= return <= 200)
fn bad(x: i32) -> i32 {
    return x
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
| 通常 | `[contract.ensures]` | 3 | 12 |

### flow-135 失敗する関数のor代替値合流後は@ensuresを持ち越さない

主根拠：`[contract.ensures]`。
補助根拠：`[contract.requires]`。

`@ensures(return > 0)` の失敗する関数で `fail` 経路は検査しないが、呼び出し側で `make_pos(x) or 0` と代替値に合流すると `x > 0` は持ち越さない。

**ソース：**

```suz
error Fail { Bad }

@ensures(return > 0)
fn make_pos(x: i32) -> i32 or Fail {
    if x > 0 {
        return x
    }
    fail Fail.Bad
}

@requires(n > 0)
fn need_pos(n: i32) -> u8 {
    return 0
}

fn main() -> u8 {
    fix v = make_pos(5) or 0
    return need_pos(v)
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[contract.requires]` | 18 | 12 |

### flow-136 一般のbool状態条件をifガードで証明して@requiresへ渡す

主根拠：`[contract.proof]`。
補助根拠：`[contract.requires]`。

`if v.active` の直後に `@requires(v.active)` の関数を呼ぶと、構文一致で証明済みとなり受理する。

**ソース：**

```suz
struct Voice {
    active: bool,
}

@requires(v.active)
fn play(v: ref Voice) -> u8 {
    return 0
}

fn main() -> u8 {
    fix v = new Voice { active: true }
    if v.active {
        return play(v)
    }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-137 共有参照先に依存する条件は関数呼び出しをまたぐと忘れる

主根拠：`[contract.proof]`。
補助根拠：`[contract.requires]`。

共有参照先 `v.active` の条件は、別の関数呼び出しをまたぐと忘れられるため、後続の `@requires(v.active)` 呼び出しはコンパイルエラーとなる。

**ソース：**

```suz
struct Voice {
    active: bool,
}

fn touch() -> void {
    return
}

@requires(v.active)
fn play(v: ref Voice) -> u8 {
    return 0
}

fn main() -> u8 {
    fix v = new Voice { active: true }
    if v.active {
        touch()
        return play(v)
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
| 通常 | `[contract.requires]` | 18 | 16 |

### flow-138 通常のifガードでも算術とasの範囲を狭めてorを省略できる

主根拠：`[arith.width]`。
補助根拠：`[contract.proof]`。

契約に限らず、通常の `if 1 <= n <= 100` の本体内でも `n` の範囲が狭まり、`(n * 2) as u8` の `or` を省略できる。

**ソース：**

```suz
fn double_if_small(n: i32) -> u8 {
    if 1 <= n <= 100 {
        return (n * 2) as u8
    }
    return 0
}

fn main() -> u8 {
    if double_if_small(25) == 50 && double_if_small(200) == 0 {
        return 0
    }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-139 契約を持つ関数を関数値にすると拒否する

主根拠：`[contract.fn-value]`。

契約を持つ関数は `fn 名前` で関数値にできない。

**ソース：**

```suz
@requires(n > 0)
fn pos(n: i32) -> i32 {
    return n
}

fn main() -> u8 {
    fix f = fn pos
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
| 通常 | `[contract.fn-value]` | 7 | 13 |

### flow-140 契約を持つメソッドは@needsの要求を満たさない

主根拠：`[contract.fn-value]`。
補助根拠：`[generic.needs-check]`。

契約を持つメソッドは `@needs` を経由した無検査呼び出しを防ぐため、`@needs` の要求を満たさない。

**ソース：**

```suz
struct Boxed {
    value: i32,
}

@requires(x > 0)
fn Boxed#set(x: i32) -> void {
    self#value = x
}

@needs T: #set(i32) -> void
fn apply[T](target: mut T) -> void {
    target#set(10)
}

fn main() -> u8 {
    fix b = new Boxed { value: 0 }
    apply(b)
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
| 通常 | `[generic.needs-check]` | 17 | 5 |
