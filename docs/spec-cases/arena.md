# 作業領域とポインタ

[目次と書式](../spec-cases.md)

### arena-001 明示した作業領域で確保して読む

主根拠：`[arena.new]`。

**ソース：**

```suz
fn main() -> u8 {
    arena { fix p = new (1 as i32)
        if .p != 1 { return 1 } }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### arena-002 通常関数も呼び出し元の作業領域へ確保する

主根拠：`[arena.default]`。

**ソース：**

```suz
fn make() -> mut i32 { return new (7 as i32) }
fn main() -> u8 {
    fix p = make()
    if .p != 7 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### arena-003 ptrは型名ではない

主根拠：`[type.name]`。

**ソース：**

```suz
fn main() -> u8 {
    fix p: ptr i32 = new (1 as i32)
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
| 通常 | `[type.name]` | 2 | 12 |

### arena-004 型のあるnullableポインタをnullから更新する

主根拠：`[arena.null]`。

**ソース：**

```suz
struct Node { value: i32, next: mut? Node }
fn main() -> u8 {
    var p: mut? Node = null
    p = new Node { value: 2, next: null }
    let n = p { if n.value != 2 { return 1 } } else { return 2 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### arena-005 nullだけでは参照先の型を決められない

主根拠：`[arena.null]`。

**ソース：**

```suz
fn main() -> u8 {
    fix p = null
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
| 通常 | `[arena.null]` | 2 | 13 |

### arena-006 非null型へnullを代入できない

主根拠：`[arena.null]`。

**ソース：**

```suz
struct Node { value: i32, next: mut? Node }
fn main() -> u8 {
    fix p: mut Node = null
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
| 通常 | `[arena.null]` | 3 | 23 |

### arena-007 nullableポインタは確認前に読めない

主根拠：`[arena.null-deref]`。

**ソース：**

```suz
fn read(p: mut? i32) -> i32 { return .p }
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
| 通常 | `[arena.null-deref]` | 1 | 38 |

### arena-008 letで得た参照から実体を更新する

主根拠：`[arena.let]`。

**ソース：**

```suz
struct Node { value: i32, next: mut? Node }
fn main() -> u8 {
    fix tail = new Node { value: 1, next: null }
    fix a = new Node { value: 0, next: tail }
    let n = a.next { n#value += 1 }
    if tail.value != 2 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### arena-009 非null参照をnullableへ渡す

主根拠：`[arena.ptr-conversion]`。

**ソース：**

```suz
fn read(p: mut? i32) -> i32 { let n = p { return .n }
    return 0 }
fn main() -> u8 {
    fix p = new (3 as i32)
    if read(p) != 3 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### arena-010 入れ子のポインタ型は拡大しない

主根拠：`[arena.ptr-conversion]`。

**ソース：**

```suz
fn read(p: mut mut? i32) -> void {}
fn main() -> u8 {
    fix p = new (3 as i32)
    fix q = new p
    read(q)
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
| 通常 | `[type.expected]` | 5 | 10 |

### arena-011 参照とnullの分岐はnullableになる

主根拠：`[arena.ptr-join]`。

**ソース：**

```suz
struct Node { value: i32, next: mut? Node }
fn choose(ready: bool, a: mut Node) -> mut? Node { return ifexpr ready { a } else { null } }
fn main() -> u8 {
    fix p = new Node { value: 3, next: null }
    let n = choose(true, p) { if n.value != 3 { return 1 } } else { return 2 }
    let n = choose(false, p) { return 3 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### arena-012 必須の再帰ポインタは初期化できない

主根拠：`[arena.recursive-struct]`。

**ソース：**

```suz
struct Node { next: mut Node }
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
| 通常 | `[arena.recursive-struct]` | 1 | 15 |

### arena-013 nullableの再帰ポインタを終端にする

主根拠：`[arena.recursive-struct]`。

**ソース：**

```suz
struct Node { value: i32, next: mut? Node }
fn main() -> u8 {
    fix p = new Node { value: 3, next: null }
    if p.value != 3 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### arena-014 returnの値は回収前に読む

主根拠：`[arena.exit]`。

**ソース：**

```suz
fn read() -> i32 { arena { fix p = new (9 as i32)
        return .p } }
fn main() -> u8 {
    if read() != 9 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### arena-015 内側の実体を外側の変数へ残さない

主根拠：`[arena.lifetime]`。

**ソース：**

```suz
fn main() -> u8 {
    var saved: mut? i32 = null
    arena { saved = new (1 as i32) }
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
| 通常 | `[arena.store]` | 3 | 13 |

### arena-016 構造体の全体代入でも短命な参照を残せない

主根拠：`[arena.aggregate]`。

**ソース：**

```suz
struct Node { value: i32, next: mut? Node }
struct Holder { item: mut? Node }
struct Outer { holder: Holder }
fn main() -> u8 {
    fix outer = new Outer { holder: Holder { item: null } }
    arena { fix inner = new Node { value: 1, next: null }
        outer#holder = Holder { item: inner } }
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
| 通常 | `[arena.store]` | 7 | 9 |

### arena-017 先頭へノードを追加する関数

主根拠：`[arena.default]`。

**ソース：**

```suz
struct Node { value: i32, next: mut? Node }
fn prepend(value: i32, tail: mut? Node) -> mut Node { return new Node { value: value, next: tail } }
fn main() -> u8 {
    fix head = prepend(2, prepend(1, null))
    let n = head.next { if n.value != 1 { return 1 } } else { return 2 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### arena-018 確保する通常関数も関数値にできる

主根拠：`[arena.function-value]`。

**ソース：**

```suz
fn make() -> mut i32 { return new (7 as i32) }
fn main() -> u8 {
    fix create = fn make
    fix p = create()
    if .p != 7 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### arena-019 同じ条件で選ぶ参照の寿命を保証する

主根拠：`[arena.objection]`。

**ソース：**

```suz
struct Node { value: i32, next: mut? Node }
fn connect(flag: bool) -> void {
    fix outer = new Node { value: 1, next: null }
    arena {
        fix inner = new Node { value: 2, next: null }
        fix destination = ifexpr flag { outer } else { inner }
        fix source = ifexpr flag { outer } else { inner }
        objection [両方とも同じ条件で同じ実体を選ぶ] {
            let p = destination { p#next = source }
        }
    }
}
fn main() -> u8 {
    connect(true)
    connect(false)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### arena-020 証明済みの寿命違反はobjectionでも解除しない

主根拠：`[arena.objection-scope]`。

**ソース：**

```suz
struct Node { value: i32, next: mut? Node }
fn main() -> u8 {
    fix outer = new Node { value: 1, next: null }
    arena { fix inner = new Node { value: 2, next: null }
        objection [参照先の寿命を保証する] { outer#next = inner } }
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
| 通常 | `[arena.store]` | 5 | 35 |

### arena-021 反復ごとに回収しない確保を警告する

主根拠：`[arena.loop-allocation]`。

**ソース：**

```suz
fn main() -> u8 {
    for i in 0..<2 {
        fix p = new (1 as i32)
    }
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
| 通常 | `[arena.loop-allocation]` | 3 | 17 |

### arena-022 有限の反復で確保の警告を抑える

主根拠：`[arena.loop-allocation]`。

**ソース：**

```suz
fn main() -> u8 {
    for i in 0..<2 {
        fix p = new (1 as i32) allow [arena.loop-allocation: 反復回数は2回に限る]
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### arena-023 refポインタは読み取りと内側の可変参照を通した書き込みを許す

主根拠：`[arena.ptr-type]`。
補助根拠：`[arena.ptr-operations]`。

**ソース：**

```suz
struct Node { value: i32 }
struct Holder { target: mut Node, items: #[i32] }
fn Node#bump() -> void { self#value += 1 }
fn main() -> u8 {
    arena {
        fix node = new Node { value: 10 }
        fix r: ref Node = new Node { value: 1 }
        if r.value != 1 { return 1 }
        fix items: #[i32] = new #[2, 3]
        fix ref_arr: ref #[i32] = new items
        (.ref_arr)#[0] = 20
        fix holder: ref Holder = new Holder { target: node, items: items }
        holder.target#bump()
        holder.items#[1] = 30
        if node.value != 11 || items.[0] != 20 || items.[1] != 30 { return 2 }
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### arena-024 refポインタの参照先全体を上書きできない

主根拠：`[arena.ptr-operations]`。

**ソース：**

```suz
struct Node { value: i32 }
fn main() -> u8 {
    arena {
        fix p: ref Node = new Node { value: 1 }
        #p = Node { value: 2 }
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
| 通常 | `[arena.ptr-operations]` | 5 | 12 |

### arena-025 refポインタ経由でフィールドを書き換えられない

主根拠：`[arena.ptr-operations]`。

**ソース：**

```suz
struct Node { value: i32 }
fn main() -> u8 {
    arena {
        fix p: ref Node = new Node { value: 1 }
        p#value = 2
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
| 通常 | `[arena.ptr-operations]` | 5 | 9 |

### arena-026 refポインタ経由で書き換えメソッドを呼べない

主根拠：`[method.write-target]`。

**ソース：**

```suz
struct Node { value: i32 }
fn Node#bump() -> void { self#value += 1 }
fn main() -> u8 {
    arena {
        fix p: ref Node = new Node { value: 1 }
        p#bump()
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
| 通常 | `[method.write-target]` | 6 | 9 |

### arena-027 refポインタからmutポインタへ暗黙変換できない

主根拠：`[arena.ptr-conversion]`。
補助根拠：`[type.expected]`。

**ソース：**

```suz
struct Node { value: i32 }
fn main() -> u8 {
    arena {
        fix r: ref Node = new Node { value: 1 }
        fix m: mut Node = r
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
| 通常 | `[type.expected]` | 5 | 27 |

### arena-028 mut #[T]からmut .[T]へ暗黙変換できない

主根拠：`[array.readonly-conversion]`。
補助根拠：`[arena.ptr-conversion]`・`[type.expected]`。

**ソース：**

```suz
fn main() -> u8 {
    arena {
        fix a: #[i32] = new #[1, 2]
        fix p: mut #[i32] = new a
        fix q: mut .[i32] = p
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
| 通常 | `[type.expected]` | 5 | 29 |
