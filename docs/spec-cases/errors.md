# エラー型と失敗する関数

### err-001 エラー値の網羅と表示

主根拠：`[error.ops]`。

**ソース：**

```suz
error ReadError { Missing, Denied }
fn main() -> u8 {
    fix e = ReadError.Denied
    log info `${e}`
    return matchexpr e {
        ReadError.Missing => 1
        ReadError.Denied => 0
    }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info err-001.suz:4: Denied
```

### err-002 失敗の伝播と代替値

主根拠：`[err.or-fail]`。

**ソース：**

```suz
error ReadError { Missing, Denied }
fn read() -> u8 or ReadError.Missing { fail ReadError.Missing }
fn forward() -> u8 or ReadError {
    fix value = read() or fail
    return value
}
fn main() -> u8 { return forward() or 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### err-003 voidの失敗を理由付きで無視する

主根拠：`[err.or-ignore]`。

理由の末尾の閉じ括弧は関数本体を閉じる。理由自体は出力しない。

**ソース：**

```suz
error SendError { Offline }
fn send() -> void or SendError { fail SendError.Offline }
fn notify() -> void { send() or ignore: オフライン時は通知を省略 }
fn main() -> u8 {
    notify()
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### err-004 呼び出しの失敗を処理しない

主根拠：`[err.must-handle]`。

**ソース：**

```suz
error ReadError { Missing }
fn read() -> u8 or ReadError { fail ReadError.Missing }
fn main() -> u8 {
    read()
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
| 通常 | `[err.must-handle]` | 4 | 5 |

### err-005 宣言していないエラーを返す

主根拠：`[err.fail]`。

**ソース：**

```suz
error ReadError { Missing, Denied }
fn read() -> u8 or ReadError.Missing {
    fail ReadError.Denied
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
| 通常 | `[err.fail]` | 3 | 10 |

### err-006 型全体とバリアントの重複指定

主根拠：`[err.fn-return]`。

**ソース：**

```suz
error ReadError { Missing }
fn read() -> u8 or ReadError, ReadError.Missing { return 0 }
fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[err.fn-return]` | 2 | 31 |
