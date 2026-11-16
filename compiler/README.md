# Suzコンパイラ

開発にはRust 1.100.0を使う。リポジトリ直下の `rust-toolchain.toml` で、ツールチェーンとrustfmt・Clippyを指定する。言語エディションは2024とする。

言語の規則は[仕様](../docs/spec.md)、検証対象は[仕様例集](../docs/spec-cases.md)に記載する。

単一バイナリとして提供するコマンドの契約・対応環境・配布方針は[ツール仕様](../docs/tool.md)で定める。

整数演算の検査、テキストプール、環境層の実装・単体検証は[ランタイム](../runtime/README.md)を参照する。

## ビルドと実行

コンパイラのビルドにはClangと `ar` も使う。`build.rs` が同梱のRyu、浮動小数点の表示処理、textの解析処理を静的にリンクする。定数評価と生成プログラムで同じ表示・解析処理を使う。`CC` と `AR` で各ツールの実行ファイルを指定できる。

```sh
cargo run -- build sample.suz -o sample -O2
./sample
cargo run -- run sample.suz
```

`build` と `run` は外部のClang・リンカー・SDKを使う。`--cc PATH` でClangを、`--env FILE` で環境層を指定できる。最適化は `-O0`（既定）または `-O2` を選ぶ。

`build`・`run`・`emit-c` は `--text-pool-limit SIZE` でテキストプールの上限を指定できる。既定値は `1G`。バイト数、`K`・`M`・`G` 付きの値、`unlimited` を受け付ける。設定は生成プログラムに組み込む。

`build` の出力先は新規ファイルとする。既存の通常ファイルを置き換える場合は `--force` を指定する。入力ソースや環境層と同じファイルは置き換えられない。

`run` は呼び出し元の作業ディレクトリと標準入出力を引き継ぎ、実行後に一時ファイルを削除する。生成プログラムの終了コード・終了シグナルを呼び出し元へ伝える。

## ソースの検査

リポジトリのルートから実行する。

```sh
cargo run -- check sample.suz
cargo run -- check sample.suz --warn-unhandled-arithmetic
cargo run -- --help
cargo run -- --version
cargo run -- --licenses
```

`check` は外部のCコンパイラ・SDKを使わずにソースを検査する。整数演算の診断設定には `--warn-unhandled-arithmetic` または `--deny-unhandled-arithmetic` を指定できる。

終了コードと診断形式は[ツール仕様](../docs/tool.md#5-メッセージと終了)に従う。

## C一式の出力

```sh
cargo run -- emit-c sample.suz -o generated
cd generated
./scripts/build-c.sh program.c -o sample -O2
./sample
```

`emit-c` は新規ディレクトリへ、生成C・ランタイム・Ryu・ライセンス・ビルドスクリプトを出力する。出力自体には外部ツールを使わない。`--env FILE` を指定すると、そのCソースを `runtime/suz_env.c` として収録する。出力先が既に存在する場合はエラーとなる。

## 検証

```sh
cargo test
```

内部実装の説明は、非公開項目も含めたrustdocで読める。生成後に `target/doc/suz/index.html` を開く。CIとリリース検査でも、リンク切れなどの警告をエラーとして生成を確認する。

```sh
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --document-private-items
```

コードのコメント・rustdocとMarkdown文書は日本語で書く。コマンドの出力と診断メッセージは英語とする。

リリース前は、書式・Clippy・rustdocと、最適化・サニタイザー・大量入力を含む全検査を実行する。

```sh
./scripts/test-release.sh
```

字句・構文の検査、例集の書式検査、生成したCを実行して戻り値を確認するテストを実行する。Cコンパイラは `clang` を使う。`CC` には別のCコンパイラの実行ファイルを指定できる。

例集から選んだ入力をコンパイル・実行する手順は [テスト基盤](tests/README.md) に記載している。

## ライセンス

Suzは[MIT](../LICENSE-MIT)または[Apache-2.0](../LICENSE-APACHE)のいずれかを選択して利用できる。著作権者はTAPPOY G.K.とする。Ryuには[同梱ライブラリのライセンス](../runtime/third_party/ryu/README.md)を適用する。
