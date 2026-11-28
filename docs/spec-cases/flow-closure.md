# 仕様例：関数と制御（クロージャ）

### flow-092 値を捕捉したクロージャを渡す
- 主根拠：`[closure.expression]`
- 結果：正常終了

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

### flow-093 クロージャを変数に保存しない
- 主根拠：`[closure.place]`
- 結果：コンパイルエラー `[closure.place]` 2:13

```suz
fn main() -> u8 {
    fix f = fn []() -> i32 { return 1 }
    return 0
}
```

### flow-094 クロージャを丸括弧で包まない
- 主根拠：`[closure.place]`
- 結果：コンパイルエラー `[closure.place]` 4:18

```suz
fnshape Once() -> i32
fn run(f: fn Once) -> i32 { return f() }
fn main() -> u8 {
    fix n = run((fn []() -> i32 { return 1 }))
    return 0
}
```

### flow-095 外側の変数を暗黙に捕捉しない
- 主根拠：`[closure.scope]`
- 結果：コンパイルエラー `[closure.scope]` 5:41

```suz
fnshape Once() -> i32
fn run(f: fn Once) -> i32 { return f() }
fn main() -> u8 {
    fix scale: i32 = 2
    fix n = run(fn []() -> i32 { return scale })
    return 0
}
```

### flow-096 fixを参照キャプチャーしない
- 主根拠：`[closure.capture-list]`
- 結果：コンパイルエラー `[closure.capture-list]` 5:21

```suz
fnshape Once() -> i32
fn run(f: fn Once) -> i32 { return f() }
fn main() -> u8 {
    fix scale: i32 = 2
    fix n = run(fn [var scale]() -> i32 { return scale })
    return 0
}
```

### flow-097 同じ名前を2回捕捉しない
- 主根拠：`[closure.capture-list]`
- 結果：コンパイルエラー `[closure.capture-list]` 5:28

```suz
fnshape Once() -> i32
fn run(f: fn Once) -> i32 { return f() }
fn main() -> u8 {
    fix scale: i32 = 2
    fix n = run(fn [scale, scale]() -> i32 { return scale })
    return 0
}
```

### flow-098 トップレベルの定数を捕捉一覧へ書かない
- 主根拠：`[closure.capture-list]`
- 結果：コンパイルエラー `[closure.capture-list]` 5:21

```suz
fix SCALE: i32 = 2
fnshape Once() -> i32
fn run(f: fn Once) -> i32 { return f() }
fn main() -> u8 {
    fix n = run(fn [SCALE]() -> i32 { return SCALE })
    return 0
}
```

### flow-099 値キャプチャーは作成時の値を保つ
- 主根拠：`[closure.capture-value]`
- 結果：正常終了

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

### flow-100 値キャプチャーを書き換えない
- 主根拠：`[closure.capture-value]`
- 結果：コンパイルエラー `[closure.capture-value]` 5:39

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

### flow-101 参照キャプチャーから外側を更新する
- 主根拠：`[closure.capture-reference]`
- 結果：正常終了

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

### flow-102 クロージャ自身を匿名汎用にしない
- 主根拠：`[closure.generic]`
- 結果：コンパイルエラー `[closure.generic]` 4:24

```suz
fnshape Calc(i32) -> i32
fn apply(f: fn Calc, value: i32) -> i32 { return f(value) }
fn main() -> u8 {
    fix result = apply(fn [](n: int) -> i32 { return 0 }, 3)
    return 0
}
```
