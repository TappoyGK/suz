# Suz 🛎️

Suzは、Cを生成する静的型付けのプログラミング言語である。

名前は日本語の「鈴」に由来し、「すず（Suzu）」と発音する。

## 目的

ゲームとシンセサイザーの開発に特化した言語と環境を提供する。

## 目標

次の順に優先する。

1. ゲームとシンセサイザーの開発速度を上げる。
2. 手書きのCに近い実行速度を実現する。

## メモリ管理

Suzは、`new`で確保する実体を作業領域（arena）に置き、その領域を抜けるときにまとめて回収する。個々の実体を解放する操作は書かない。

代表的な動的確保の扱いを比較すると、次のようになる。

| 項目 | C（`malloc`・`free`） | Rust（`Box`） | Suz（`new`・`arena`） |
|---|---|---|---|
| 回収の時点 | プログラマーが`free`を呼ぶとき | 所有者が破棄されるとき | 作業領域を抜けるとき |
| 意識すること | 誰が、いつ解放するか | 誰が所有し、誰が借用するか | どの作業領域に置き、いつまで使うか |

`main`は既定の作業領域を持つ。関数は呼び出された時点の作業領域へ確保し、関数内で`arena`を開いた場合は、その内側では新しい領域へ確保する。値を返すかどうかで確保先は変わらない。

```suz
fn main() -> u8 {
    fix ptr saved = new 10 // mainの終了まで保持する実体
    arena {
        fix ptr temporary = new 20
        fix total = .saved + .temporary
        log info `${total}`
    } // temporaryが指す実体を回収する
    fix value = .saved // 外側の実体は引き続き使える
    log info `${value}`
    return 0
}
```

通常の寿命検査は、実体を確保した作業領域が終了した後も使える場所へ、その実体への参照を残すことを拒否する。プログラマーの保証を使う`objection`とFFIの契約については、仕様書の[理由付きの寿命保証](docs/spec.md#理由付きの寿命保証)と[安全性](docs/spec.md#12-安全性)を参照する。

ループ内で確保した実体については、参照が反復の外へ残らず、かつ確保先の作業領域が反復ごとに回収されない場合に警告する（[arena.loop-allocation]）。詳しい規則は[作業領域とポインタ](docs/spec.md#813-作業領域とポインタ)、初期容量・追加容量・上限の指定は[作業領域の容量](docs/tool.md#32-作業領域の容量)を参照する。

## 導入

配布物は実行ファイル `suz` 1つである。実行権限を付け、`PATH` の通ったディレクトリに置く。

```sh
chmod +x suz
suz --version
```

対象はmacOS・Apple Siliconである。対応するOS・Cコンパイラ・SDKは[ツール仕様](docs/tool.md#1-配布と外部ツール)で定める。

プログラムのビルドと実行には、外部のClang・リンカー・macOS SDKを使う。Xcode Command Line Toolsで用意する場合は、次を実行する。

```sh
xcode-select --install
```

ソースの検査とC一式の出力には、外部ツールの用意は不要である。

## 使い方

次を `hello.suz` に保存する。

```suz
fn main() -> u8 {
    log info `Hello, Suz!`
    return 0
}
```

```sh
suz check hello.suz
suz run hello.suz
```

ログは標準エラーに出力される。

```text
info hello.suz:2: Hello, Suz!
```

実行ファイルを残す場合は `build` を使う。出力先には新規ファイルを指定する。

```sh
suz build hello.suz -o hello -O2
./hello
```

C一式を取り出す場合は、新規ディレクトリを指定する。ランタイム・環境層・Ryu・ライセンス・ビルドスクリプトも一緒に出力される。

```sh
suz emit-c hello.suz -o generated
```

コマンドとオプションは `suz --help`、`suz build --help` などで確認できる。

## 文書

[文書目録](docs/README.md)から、言語仕様・ランタイム仕様・ツール仕様・仕様例集・リリース手順を参照できる。

## 開発

Rustのツールチェーンは `rust-toolchain.toml` で指定する。リポジトリのルートでビルドと通常テストを実行する。

```sh
cargo build --release --locked
cargo test --locked
```

ビルドしたコマンドは `target/release/suz` に生成される。詳しい利用方法は[コンパイラ](compiler/README.md)、検証方法は[テスト基盤](compiler/tests/README.md)を参照する。

## ライセンス

Copyright (c) 2026 TAPPOY G.K.

Suzは[MIT](LICENSE-MIT)または[Apache-2.0](LICENSE-APACHE)のいずれかを選択して利用できる。Ryuには[同梱ライブラリのライセンス](runtime/third_party/ryu/README.md)を適用する。ライセンス本文は `suz --licenses` でも表示できる。
