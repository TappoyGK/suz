# 仕様例：配列（SoA 配列）

### array-094 多次元SoA配列の部分配列・切り出し・反復
- 主根拠：`[array.soa-type]`
- 補助根拠：`[array.soa-column]`、`[array.soa-access]`、`[array.iteration]`
- 結果：正常終了

```suz
struct Cell { hp: i32, mp: i32 }

fn sum_hp(grid: soa .[Cell; h, w]) -> i32 {
    var total: i32 = 0
    for row in grid {
        for cell in row {
            total +%= cell.hp
        }
    }
    return total
}

fn main() -> u8 {
    fix grid = new soa #[Cell { hp: 1, mp: 2 }; 2, 3]
    grid.[0].[1]#hp = 10
    grid.[1].[2]#hp = 20
    if grid.hp.[0].[1] != 10 || grid.hp.[1].[2] != 20 { return 1 }
    fix row1: soa #[Cell] = grid.[1]
    fix slice = row1.[1:3]
    var rev_sum: i32 = 0
    rev c in slice {
        rev_sum +%= c.hp
    }
    if rev_sum != 21 { return 3 }
    if sum_hp(grid) != 34 { return 4 }
    return 0
}
```

### array-095 列挙型添字SoA配列と定数SoA配列の列取り出し
- 主根拠：`[array.soa-type]`
- 補助根拠：`[array.enum-type]`、`[array.constant]`、`[array.soa-column]`
- 結果：正常終了

```suz
enum Axis: u8 { X, Y }
struct Pair { a: i32, b: i32 }

fix CONST_PAIRS: soa .[Pair] = soa .[
    Pair { a: 10, b: 1 },
    Pair { a: 20, b: 2 },
]
fix CONST_A: .[i32] = CONST_PAIRS.a

fn main() -> u8 {
    fix by_axis = new soa #[
        Axis.X: Pair { a: 3, b: 4 },
        Axis.Y: Pair { a: 5, b: 6 },
    ]
    fix rep = new soa #[Pair { a: 7, b: 8 }; Axis]
    if by_axis.a.[Axis.X] != 3 || by_axis.[Axis.Y].b != 6 { return 1 }
    if rep.b.[Axis.X] != 8 || len rep != 2 { return 2 }
    if CONST_A.[0] != 10 || CONST_A.[1] != 20 { return 3 }
    return 0
}
```

### array-096 汎用構造体と汎用関数でのSoA配列
- 主根拠：`[array.soa-type]`
- 補助根拠：`[generic.struct]`、`[generic.inference]`
- 結果：正常終了

```suz
struct Pair[T] { first: T, second: T }

fn sum_first(items: soa .[Pair[i32]; n]) -> i32 {
    var acc: i32 = 0
    for i in 0..<n {
        acc +%= items.first.[i]
    }
    return acc
}

fn len_of[T](items: soa .[Pair[T]]) -> len_t {
    return len items
}

fn main() -> u8 {
    fix items = new soa #[
        Pair[i32] { first: 4, second: 1 },
        Pair[i32] { first: 6, second: 2 },
    ]
    if sum_first(items) != 10 { return 1 }
    if len_of(items) != 2 { return 2 }
    return 0
}
```

### array-097 構造体以外を要素にするSoA配列はコンパイルエラーになる
- 主根拠：`[array.soa-type]`
- 結果：コンパイルエラー `[array.soa-type]` 2:16

```suz
fn main() -> u8 {
    fix a: soa #[i32] = new soa #[1, 2]
    return 0
}
```

### array-098 SoA配列と通常配列の相互の暗黙変換はコンパイルエラーになる
- 主根拠：`[array.soa-type]`
- 補助根拠：`[type.expected]`
- 結果：コンパイルエラー `[type.expected]` 4:23

```suz
struct Point { x: i32, y: i32 }
fn main() -> u8 {
    fix a: soa #[Point] = new soa #[Point { x: 1, y: 2 }]
    fix b: #[Point] = a
    return 0
}
```

### array-099 newなしの可変SoA配列初期化子はコンパイルエラーになる
- 主根拠：`[array.soa-type]`
- 補助根拠：`[array.new]`
- 結果：コンパイルエラー `[array.new]` 3:13

```suz
struct Point { x: i32, y: i32 }
fn main() -> u8 {
    fix a = soa #[Point { x: 1, y: 2 }]
    return 0
}
```

### array-100 汎用関数の型引数に非構造体を与えたSoA配列はコンパイルエラーになる
- 主根拠：`[array.soa-type]`
- 結果：コンパイルエラー `[array.soa-type]` 5:13

```suz
fn f[T](a: soa .[T]) -> len_t {
    return len a
}
fn main() -> u8 {
    fix x = f[i32](new #[1])
    return 0
}
```

### array-101 SoA配列の入れ子列・マスク付き一括書き込み・要素とフィールドの読み書き
- 主根拠：`[array.soa-column]`
- 補助根拠：`[array.soa-type]`、`[array.soa-access]`、`[array.bulk]`、`[array.dimension-origin]`
- 結果：正常終了

```suz
struct Vec2 { x: f32, y: f32 }
struct Particle { pos: Vec2, vel: Vec2, life: f32 }

fn step(particles: soa #[Particle; n], dt: f32) -> void {
    particles.pos.x#[particles.life > 0.0] += particles.vel.x * dt
    particles.pos.y#[particles.life > 0.0] += particles.vel.y * dt
    for i in 0..<n {
        if particles.[i].life > 0.0 {
            particles.[i]#life -= dt
        }
    }
}

fn main() -> u8 {
    fix p = new soa #[
        Particle { pos: Vec2 { x: 1.0, y: 2.0 }, vel: Vec2 { x: 10.0, y: 20.0 }, life: 2.0 },
        Particle { pos: Vec2 { x: 3.0, y: 4.0 }, vel: Vec2 { x: 10.0, y: 20.0 }, life: 0.0 },
    ]
    step(p, 0.5)
    if p.[0].pos.x != 6.0 || p.[0].pos.y != 12.0 || p.[0].life != 1.5 { return 1 }
    if p.[1].pos.x != 3.0 || p.[1].pos.y != 4.0 || p.[1].life != 0.0 { return 2 }
    fix pos_soa: soa #[Vec2] = p.pos
    if pos_soa.x.[0] != 6.0 { return 3 }
    p.[1].pos#x += 1.0
    p.[1].pos#y += 2.0
    if p.[1].pos.x != 4.0 || p.[1].pos.y != 6.0 { return 4 }
    fix snap = p.[0]
    p#[1] = snap
    if p.[1].pos.x != 6.0 || p.[1].life != 1.5 { return 5 }
    return 0
}
```

### array-102 長さ0・nil許容・読み取り専用変換とSoA配列の列取り出し
- 主根拠：`[array.soa-column]`
- 補助根拠：`[array.readonly-conversion]`、`[array.value]`、`[arena.let]`
- 結果：正常終了

```suz
struct Item { v: i32 }

fn read_opt(a: soa .[Item]?) -> i32 {
    let items = a {
        fix col = items.v
        return ifexpr len col == 0 { 10 } else { col.[0] or 0 }
    }
    return 20
}

fn main() -> u8 {
    fix empty: soa #[Item] = new soa #[]
    fix empty_v = empty.v
    if len empty_v != 0 { return 1 }
    if read_opt(empty) != 10 { return 2 }
    fix none: soa #[Item]? = nil
    if read_opt(none) != 20 { return 3 }
    fix one = new soa #[Item { v: 42 }]
    fix ro: soa .[Item] = one
    if ro.v.[0] != 42 { return 4 }
    return 0
}
```

### array-103 SoA配列の双方向の別名可視性・重なり切り出し一括書き込み・混在一括書き込み・初期値の1回評価
- 主根拠：`[array.soa-column]`
- 補助根拠：`[array.soa-access]`、`[array.bulk-eval]`、`[array.new]`
- 結果：正常終了

```suz
struct Node { v: i32, tag: i32 }

fn make(counter: mut i32) -> Node {
    #counter +%= 1
    return Node { v: .counter, tag: 9 }
}

fn bump_col(xs: #[i32; n]) -> void {
    for i in 0..<n {
        xs#[i] +%= 1
    }
}

fn main() -> u8 {
    fix c = new (0 as i32)
    fix nodes = new soa #[make(c); 4]
    if .c != 1 { return 1 }
    fix alias = nodes.v
    nodes#[0] = Node { v: 10, tag: 1 }
    nodes#[1] = Node { v: 20, tag: 2 }
    nodes#[2] = Node { v: 30, tag: 3 }
    nodes#[3] = Node { v: 40, tag: 4 }
    if alias.[2] != 30 { return 2 }
    // 列の別名および関数へ渡した列からの書き込みも元の SoA から見える。
    alias#[0] = 15
    bump_col(nodes.v)
    if nodes.[0].v != 16 || nodes.[1].v != 21 { return 3 }
    nodes.v#[1:4] = nodes.v.[0:3]
    if nodes.v.[0] != 16 || nodes.v.[1] != 16 || nodes.v.[2] != 16 || nodes.v.[3] != 16 {
        return 4
    }
    nodes#[1:4] = nodes.[0:3]
    if nodes.[3].tag != 1 { return 5 }
    // 同じ要素型の通常配列と SoA 配列を混ぜた一括書き込み。
    fix aos = new #[Node { v: 7, tag: 8 }; 4]
    nodes#[] = aos
    if nodes.[2].v != 7 || nodes.[2].tag != 8 { return 6 }
    return 0
}
```

### array-104 nil許容のSoA配列からlet確認なしで列を取り出すとコンパイルエラーになる
- 主根拠：`[array.soa-column]`
- 補助根拠：`[array.value]`、`[arena.let]`
- 結果：コンパイルエラー `[array.value]` 3:16

```suz
struct Point { x: i32, y: i32 }
fn f(a: soa #[Point]?) -> void {
    fix xs = a.x
}
fn main() -> u8 { return 0 }
```

### array-105 読み取り専用SoA配列から得た列への書き込みはコンパイルエラーになる
- 主根拠：`[array.soa-column]`
- 補助根拠：`[array.readonly]`、`[array.write]`
- 結果：コンパイルエラー `[array.readonly]` 3:9

```suz
struct Point { x: i32, y: i32 }
fn f(a: soa .[Point]) -> void {
    a.x#[0] = 1
}
fn main() -> u8 { return 0 }
```

### array-106 SoA配列から取り出した列は元の作業領域を超えて保持できない
- 主根拠：`[array.soa-column]`
- 補助根拠：`[array.lifetime]`、`[arena.store]`
- 結果：コンパイルエラー `[arena.store]` 6:9

```suz
struct Point { x: i32, y: i32 }
fn main() -> u8 {
    var escaped: #[i32]? = nil
    arena {
        fix p = new soa #[Point { x: 1, y: 2 }]
        escaped = p.x
    }
    return 0
}
```
