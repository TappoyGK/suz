# エラー型と失敗する関数

### err-001 エラー値の網羅と表示
- 主根拠：`[error.ops]`
- 結果：正常終了

```suz
error ReadError { Missing, Denied }
fn main() -> u8 {
    fix e = ReadError.Denied
    log info "${e}"
    return matchexpr e {
        ReadError.Missing => 1
        ReadError.Denied => 0
    }
}
```

**標準エラー：**

```text
info err-001.suz:4: Denied
```

### err-002 失敗の伝播と代替値
- 主根拠：`[err.or-fail]`
- 結果：正常終了

```suz
error ReadError { Missing, Denied }
fn read() -> u8 or ReadError.Missing { fail ReadError.Missing }
fn forward() -> u8 or ReadError {
    fix value = read() or fail
    return value
}
fn main() -> u8 { return forward() or 0 }
```

### err-003 voidの失敗を理由付きで無視する
- 主根拠：`[err.or-ignore]`
- 結果：正常終了

理由の末尾の閉じ括弧は関数本体を閉じる。理由自体は出力しない。

```suz
error SendError { Offline }
fn send() -> void or SendError { fail SendError.Offline }
fn notify() -> void { send() or ignore [オフライン時は通知を省略] }
fn main() -> u8 {
    notify()
    return 0
}
```

### err-004 呼び出しの失敗を処理しない
- 主根拠：`[err.must-handle]`
- 結果：コンパイルエラー `[err.must-handle]` 4:5

```suz
error ReadError { Missing }
fn read() -> u8 or ReadError { fail ReadError.Missing }
fn main() -> u8 {
    read()
    return 0
}
```

### err-005 宣言していないエラーを返す
- 主根拠：`[err.fail]`
- 結果：コンパイルエラー `[err.fail]` 3:10

```suz
error ReadError { Missing, Denied }
fn read() -> u8 or ReadError.Missing {
    fail ReadError.Denied
}
fn main() -> u8 { return 0 }
```

### err-006 型全体とバリアントの重複指定
- 主根拠：`[err.fn-return]`
- 結果：コンパイルエラー `[err.fn-return]` 2:31

```suz
error ReadError { Missing }
fn read() -> u8 or ReadError, ReadError.Missing { return 0 }
fn main() -> u8 { return 0 }
```

### err-007 値を返さない呼び出しの失敗を伝播する
- 主根拠：`[err.or-fail]`
- 補助根拠：`[err.or-return]`・`[expr.or-return-position]`
- 結果：正常終了 7

成功型がvoidなら、呼び出し全体を文として書いて失敗を伝播できる。mainは伝播された失敗を処理し、成功時の後続処理には進まない。

```suz
error SendError { Offline }
fn send() -> void or SendError { fail SendError.Offline }
fn notify() -> void or SendError {
    send() or fail
}
fn main() -> u8 {
    notify() or return 7
    return 0
}
```

### err-008 エラー別の代替値とrecover
- 主根拠：`[err.or-error]`
- 補助根拠：`[err.recover]`
- 結果：正常終了 7

ブロック腕のrecoverは関数から戻らず、呼び出しの代替値を決めて後続処理へ進む。

```suz
error ReadError { Missing, Denied }
fn read() -> u8 or ReadError { fail ReadError.Denied }
fn main() -> u8 {
    fix value = read() or error {
        ReadError.Missing => 1
        ReadError.Denied => {
            fix fallback: u8 = 7
            recover fallback
        }
    }
    return value
}
```

### err-009 voidのエラー分岐から続行する
- 主根拠：`[err.or-error]`
- 結果：正常終了 3

voidの腕は理由付きで失敗を無視するか、空でないブロックの末尾へ到達して続行できる。

```suz
error SendError { Offline, Busy }
fn send() -> void or SendError { fail SendError.Busy }
fn main() -> u8 {
    var result: u8 = 0
    send() or error {
        SendError.Offline => ignore [オフライン時は通知を省略]
        SendError.Busy => { result = 3 }
    }
    return result
}
```

### err-010 エラー分岐の網羅不足
- 主根拠：`[err.or-error]`
- 結果：コンパイルエラー `[err.or-error]` 4:22

実際に返す値ではなく、呼び出し先の宣言に含まれる全エラーを処理する必要がある。

```suz
error ReadError { Missing, Denied }
fn read() -> u8 or ReadError { fail ReadError.Missing }
fn main() -> u8 {
    return read() or error { ReadError.Missing => 0 }
}
```
