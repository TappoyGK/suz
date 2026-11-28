# プログラムの構造

[目次と書式](../spec-cases.md)

### prog-001 最小のプログラム
- 主根拠：`[prog.main]`
- 補助根拠：`[prog.exit]`
- 結果：正常終了

```suz
fn main() -> u8 {
    return 0
}
```

### prog-002 mainの戻り値型
- 主根拠：`[prog.main]`
- 結果：コンパイルエラー `[prog.main]` 1:14

mainの戻り値型はu8である。

```suz
fn main() -> i32 { return 0 }
```

### prog-003 mainが存在しない
- 主根拠：`[prog.main]`
- 結果：コンパイルエラー `[prog.main]` 4:1

入口となるmainが必要。このソース全体にはhelperだけがある。

```suz
fn helper() -> u8 {
    return 0
}
```

### prog-004 mainを2つ宣言する
- 主根拠：`[prog.main]`
- 補助根拠：`[decl.duplicate]`
- 結果：コンパイルエラー `[prog.main]` 2:8

```suz
fn main() -> u8 { return 0 }
fn main() -> u8 { return 1 }
```

### prog-005 後で宣言する関数と定数を参照する
- 主根拠：`[decl.top-level]`
- 補助根拠：`[const.initializer]`
- 結果：正常終了 3

mainは後にある関数を、定数ANSWERは後にある定数BASEを参照できる。

```suz
fn main() -> u8 {
    return calculate()
}

fix ANSWER: u8 = BASE + 1
fix BASE: u8 = 2

fn calculate() -> u8 {
    return ANSWER
}
```

### prog-006 トップレベルにfixを宣言する
- 主根拠：`[decl.location]`
- 補助根拠：`[const.decl]`
- 結果：正常終了 1

トップレベルのfixで定数を宣言できる。

```suz
fix VALUE: u8 = 1

fn main() -> u8 { return VALUE }
```

### prog-007 ブロック内のfixに大文字の名前を付ける
- 主根拠：`[name.value]`
- 結果：コンパイルエラー `[name.value]` 2:9

ブロック内のfixの名前は小文字で始める。

```suz
fn main() -> u8 {
    fix VALUE: u8 = 1
    return 0
}
```

### prog-008 構造体と列挙型に同じ名前を付ける
- 主根拠：`[decl.duplicate]`
- 結果：コンパイルエラー `[decl.duplicate]` 3:8

宣言の種類が違っても、同じスコープの名前は重複できない。

```suz
enum Value: u8 { First }

struct Value {
    count: u8,
}

fn main() -> u8 { return 0 }
```

### prog-009 独立したブロックで同じ変数名を使う
- 主根拠：`[decl.shadowing]`
- 補助根拠：`[decl.local-scope]`
- 結果：正常終了 3

2つのvalueのスコープは重ならない。

```suz
fn main() -> u8 {
    var total: u8 = 0
    {
        fix value: u8 = 1
        total += value
    }
    {
        fix value: u8 = 2
        total += value
    }
    return total
}
```

### prog-010 ローカル変数のシャドーイング
- 主根拠：`[decl.shadowing]`
- 結果：コンパイルエラー `[decl.shadowing]` 4:13

内側のcountが外側の名前を隠蔽する。

```suz
fn main() -> u8 {
    fix count = 1
    {
        fix count = 2
    }
    return 0
}
```

### prog-011 ローカル変数を宣言前に参照する
- 主根拠：`[decl.local-scope]`
- 結果：コンパイルエラー `[decl.local-scope]` 2:21

firstの初期化時には、後に宣言するsecondを参照できない。

```suz
fn main() -> u8 {
    fix first: u8 = second
    fix second: u8 = 1
    return first
}
```

### prog-012 自分自身を初期値で参照する
- 主根拠：`[decl.local-scope]`
- 結果：コンパイルエラー `[decl.local-scope]` 2:21

valueは自分自身の初期値の式では参照できない。

```suz
fn main() -> u8 {
    fix value: u8 = value
    return value
}
```

### prog-013 ブロック終了後にローカル変数を参照する
- 主根拠：`[decl.local-scope]`
- 結果：コンパイルエラー `[decl.local-scope]` 5:12

valueのスコープは内側のブロックの終わりまで。

```suz
fn main() -> u8 {
    {
        fix value: u8 = 1
    }
    return value
}
```

### prog-014 定数名に小文字を使う
- 主根拠：`[name.top-level-binding]`
- 結果：コンパイルエラー `[name.top-level-binding]` 1:5

```suz
fix max_count: u8 = 3
fn main() -> u8 { return max_count }
```

### prog-015 関数名に大文字を混ぜる
- 主根拠：`[name.value]`
- 結果：コンパイルエラー `[name.value]` 1:4

```suz
fn readCount() -> u8 { return 0 }
fn main() -> u8 { return 0 }
```

### prog-016 型名で大文字を連続させる
- 主根拠：`[name.type]`
- 結果：コンパイルエラー `[name.type]` 1:8

```suz
struct IOError { code: u8 }
fn main() -> u8 { return 0 }
```

### prog-017 列挙子名にアンダースコアを使う
- 主根拠：`[name.variant]`
- 結果：コンパイルエラー `[name.variant]` 1:18

```suz
enum State: u8 { Not_Ready }
fn main() -> u8 { return 0 }
```

### prog-018 宣言の種類ごとの命名規則に従う
- 主根拠：`[name.declaration]`
- 結果：正常終了

```suz
fix MAX_COUNT: u8 = 3
enum Key: u8 { A, F1, ESC }
struct Vec2D { x2: u8, key: Key }
fn read_count(input_value: Vec2D) -> u8 { return input_value.x2 }
fn main() -> u8 {
    fix value = Vec2D { x2: MAX_COUNT, key: Key.ESC }
    var sum: u8 = 0
    for index in 0..<read_count(value) { sum += index }
    match sum {
        3 => { return 0 }
        _ => { return 1 }
    }
}
```
