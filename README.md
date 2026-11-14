# Suz

Suzは、Cを生成する静的型付けのプログラミング言語である。

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
