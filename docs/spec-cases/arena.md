# 作業領域とポインタ

[目次と書式](../spec-cases.md)

### arena-001 明示した作業領域で確保して読む
- 主根拠：`[arena.new]`
- 結果：正常終了

```suz
fn main() -> u8 {
    arena { fix p = new (1 as i32)
        if .p != 1 { return 1 } }
    return 0
}
```

### arena-002 通常関数も呼び出し元の作業領域へ確保する
- 主根拠：`[arena.default]`
- 結果：正常終了

```suz
fn make() -> mut i32 { return new (7 as i32) }
fn main() -> u8 {
    fix p = make()
    if .p != 7 { return 1 }
    return 0
}
```

### arena-003 ptrは型名ではない
- 主根拠：`[type.name]`
- 結果：コンパイルエラー `[type.name]` 2:12

```suz
fn main() -> u8 {
    fix p: ptr i32 = new (1 as i32)
    return 0
}
```

### arena-004 型のあるnilableポインタをnilから更新する
- 主根拠：`[arena.nil]`
- 結果：正常終了

```suz
struct Node { value: i32, next: mut? Node }
fn main() -> u8 {
    var p: mut? Node = nil
    p = new Node { value: 2, next: nil }
    let n = p { if n.value != 2 { return 1 } } else { return 2 }
    return 0
}
```

### arena-005 nilだけでは参照先の型を決められない
- 主根拠：`[arena.nil]`
- 結果：コンパイルエラー `[arena.nil]` 2:13

```suz
fn main() -> u8 {
    fix p = nil
    return 0
}
```

### arena-006 非nil型へnilを代入できない
- 主根拠：`[arena.nil]`
- 結果：コンパイルエラー `[arena.nil]` 3:23

```suz
struct Node { value: i32, next: mut? Node }
fn main() -> u8 {
    fix p: mut Node = nil
    return 0
}
```

### arena-007 nilableポインタは確認前に読めない
- 主根拠：`[arena.nil-deref]`
- 結果：コンパイルエラー `[arena.nil-deref]` 1:38

```suz
fn read(p: mut? i32) -> i32 { return .p }
fn main() -> u8 {

    return 0
}
```

### arena-008 letで得た参照から実体を更新する
- 主根拠：`[arena.let]`
- 結果：正常終了

```suz
struct Node { value: i32, next: mut? Node }
fn main() -> u8 {
    fix tail = new Node { value: 1, next: nil }
    fix a = new Node { value: 0, next: tail }
    let n = a.next { n#value += 1 }
    if tail.value != 2 { return 1 }
    return 0
}
```

### arena-009 非nil参照をnilableへ渡す
- 主根拠：`[arena.ptr-conversion]`
- 結果：正常終了

```suz
fn read(p: mut? i32) -> i32 { let n = p { return .n }
    return 0 }
fn main() -> u8 {
    fix p = new (3 as i32)
    if read(p) != 3 { return 1 }
    return 0
}
```

### arena-010 入れ子のポインタ型は拡大しない
- 主根拠：`[arena.ptr-conversion]`
- 結果：コンパイルエラー `[type.expected]` 5:10

```suz
fn read(p: mut mut? i32) -> void {}
fn main() -> u8 {
    fix p = new (3 as i32)
    fix q = new p
    read(q)
    return 0
}
```

### arena-011 参照とnilの分岐はnilableになる
- 主根拠：`[arena.ptr-join]`
- 結果：正常終了

```suz
struct Node { value: i32, next: mut? Node }
fn choose(ready: bool, a: mut Node) -> mut? Node { return ifexpr ready { a } else { nil } }
fn main() -> u8 {
    fix p = new Node { value: 3, next: nil }
    let n = choose(true, p) { if n.value != 3 { return 1 } } else { return 2 }
    let n = choose(false, p) { return 3 }
    return 0
}
```

### arena-012 必須の再帰ポインタは初期化できない
- 主根拠：`[arena.recursive-struct]`
- 結果：コンパイルエラー `[arena.recursive-struct]` 1:15

```suz
struct Node { next: mut Node }
fn main() -> u8 {

    return 0
}
```

### arena-013 nilableの再帰ポインタを終端にする
- 主根拠：`[arena.recursive-struct]`
- 結果：正常終了

```suz
struct Node { value: i32, next: mut? Node }
fn main() -> u8 {
    fix p = new Node { value: 3, next: nil }
    if p.value != 3 { return 1 }
    return 0
}
```

### arena-014 returnの値は回収前に読む
- 主根拠：`[arena.exit]`
- 結果：正常終了

```suz
fn read() -> i32 { arena { fix p = new (9 as i32)
        return .p } }
fn main() -> u8 {
    if read() != 9 { return 1 }
    return 0
}
```

### arena-015 内側の実体を外側の変数へ残さない
- 主根拠：`[arena.lifetime]`
- 結果：コンパイルエラー `[arena.store]` 3:13

```suz
fn main() -> u8 {
    var saved: mut? i32 = nil
    arena { saved = new (1 as i32) }
    return 0
}
```

### arena-016 構造体の全体代入でも短命な参照を残せない
- 主根拠：`[arena.aggregate]`
- 結果：コンパイルエラー `[arena.store]` 7:9

```suz
struct Node { value: i32, next: mut? Node }
struct Holder { item: mut? Node }
struct Outer { holder: Holder }
fn main() -> u8 {
    fix outer = new Outer { holder: Holder { item: nil } }
    arena { fix inner = new Node { value: 1, next: nil }
        outer#holder = Holder { item: inner } }
    return 0
}
```

### arena-017 先頭へノードを追加する関数
- 主根拠：`[arena.default]`
- 結果：正常終了

```suz
struct Node { value: i32, next: mut? Node }
fn prepend(value: i32, tail: mut? Node) -> mut Node { return new Node { value: value, next: tail } }
fn main() -> u8 {
    fix head = prepend(2, prepend(1, nil))
    let n = head.next { if n.value != 1 { return 1 } } else { return 2 }
    return 0
}
```

### arena-018 確保する通常関数も関数値にできる
- 主根拠：`[arena.function-value]`
- 結果：正常終了

```suz
fn make() -> mut i32 { return new (7 as i32) }
fn main() -> u8 {
    fix create = fn make
    fix p = create()
    if .p != 7 { return 1 }
    return 0
}
```

### arena-019 同じ条件で選ぶ参照の寿命を保証する
- 主根拠：`[arena.objection]`
- 結果：正常終了

```suz
struct Node { value: i32, next: mut? Node }
fn connect(flag: bool) -> void {
    fix outer = new Node { value: 1, next: nil }
    arena {
        fix inner = new Node { value: 2, next: nil }
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

### arena-020 証明済みの寿命違反はobjectionでも解除しない
- 主根拠：`[arena.objection-scope]`
- 結果：コンパイルエラー `[arena.store]` 5:35

```suz
struct Node { value: i32, next: mut? Node }
fn main() -> u8 {
    fix outer = new Node { value: 1, next: nil }
    arena { fix inner = new Node { value: 2, next: nil }
        objection [参照先の寿命を保証する] { outer#next = inner } }
    return 0
}
```

### arena-021 反復ごとに回収しない確保を警告する
- 主根拠：`[arena.loop-allocation]`
- 結果：正常終了 / 警告：`[arena.loop-allocation]` 3:17

```suz
fn main() -> u8 {
    for i in 0..<2 {
        fix p = new (1 as i32)
    }
    return 0
}
```

### arena-022 有限の反復で確保の警告を抑える
- 主根拠：`[arena.loop-allocation]`
- 結果：正常終了

```suz
fn main() -> u8 {
    for i in 0..<2 {
        fix p = new (1 as i32) allow [arena.loop-allocation: 反復回数は2回に限る]
    }
    return 0
}
```

### arena-023 refポインタは読み取りと内側の可変参照を通した書き込みを許す
- 主根拠：`[arena.ptr-type]`
- 補助根拠：`[arena.ptr-operations]`
- 結果：正常終了

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

### arena-024 refポインタの参照先全体を上書きできない
- 主根拠：`[arena.ptr-operations]`
- 結果：コンパイルエラー `[arena.ptr-operations]` 5:12

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

### arena-025 refポインタ経由でフィールドを書き換えられない
- 主根拠：`[arena.ptr-operations]`
- 結果：コンパイルエラー `[arena.ptr-operations]` 5:9

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

### arena-026 refポインタ経由で書き換えメソッドを呼べない
- 主根拠：`[method.write-target]`
- 結果：コンパイルエラー `[method.write-target]` 6:9

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

### arena-027 refポインタからmutポインタへ暗黙変換できない
- 主根拠：`[arena.ptr-conversion]`
- 補助根拠：`[type.expected]`
- 結果：コンパイルエラー `[type.expected]` 5:27

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
