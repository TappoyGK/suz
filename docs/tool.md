# Suz ツール仕様 v0.3.1

この文書は、`suz` コマンドの利用方法と配布物の契約を定める。言語の動作は[言語仕様](spec.md)、生成プログラムの動作は[ランタイム仕様](runtime.md)に従う。規則名には `tool.` を使う。

## 1. 配布と外部ツール

**[tool.distribution]** 配布物は、対象OS・CPU向けの実行ファイル `suz` 1つである。コンパイラ、ランタイム本体、標準環境層、Ryu、そのライセンス表示に必要なデータを内包する。実行ファイルを移動した後も、その場所や元のソースツリーに依存せず動作する。

**[tool.c-compiler]** 実行ファイルの生成には、外部のClangと対象環境のSDKを使う。`--cc PATH` を指定した場合はその実行ファイルを使い、省略した場合は `PATH` 上の `clang` を使う。指定値は1つの実行ファイルのパスとして扱う。Clangの引数はSuzが構成し、シェルを介さず起動する。対応環境は `[tool.platform]` で定める。

**[tool.prerequisites]** `build` と `run` は、処理の開始時にClang・リンカー・SDKが利用できることを確認する。不足する場合は、不足しているものと導入方法を標準エラーに表示し、終了コード2で終了する。macOSではXcode Command Line Toolsの導入方法として `xcode-select --install` を案内する。コンパイラが見つからない場合とSDKが見つからない場合は、診断を区別する。外部ツールのインストールは利用者が行う。`check` と `emit-c` は、外部ツールを用意せず実行できる。

**[tool.platform]** v0.3.1の対象環境は次のとおりである。OSとClangのバージョンは、コンパイラ・ランタイムを実際に検証した値である。

| 項目 | 対象環境 |
|---|---|
| OS | macOS 26.5（25F71） |
| CPU | Apple Silicon（arm64） |
| Cコンパイラ | Apple Clang 21.0.0（clang-2100.1.1.101） |
| SDK | 上記Clangで対象のmacOS実行ファイルをリンクできるmacOS SDK |

macOSではXcodeまたはCommand Line ToolsでClang・リンカー・SDKを用意する。

## 2. コマンド

**[tool.commands]** 次のコマンドを提供する。`INPUT` はSuzソースファイル1つである。

```text
suz build INPUT -o OUTPUT [OPTIONS]
suz run INPUT [OPTIONS]
suz check INPUT [OPTIONS]
suz emit-c INPUT -o DIRECTORY [OPTIONS]
suz --version
suz --help
suz --licenses
```

**[tool.arguments]** コマンド名の後では、入力ファイルとオプションの順序は自由である。値を取るオプションは、次の引数を値として読む。`--` より後の引数はファイル名として扱う。未知のコマンド・オプション、必須引数の欠落、入力の重複、同じオプションの重複、排他的なオプションの併用は使用方法のエラーである。引数なしの起動も使用方法のエラーとし、標準エラーへ使用方法を表示する。

### 2.1 build

**[tool.build]** 入力を検査し、Cへ変換し、同梱ランタイム・環境層とリンクした実行ファイルを `OUTPUT` に作成する。`-o` は必須である。

```sh
suz build program.suz -o program
suz build program.suz -o program -O2
```

### 2.2 run

**[tool.run]** 入力を一時領域でビルドし、生成した実行ファイルを1回実行する。生成プログラムの作業ディレクトリは、`suz run` を呼んだ作業ディレクトリである。標準入力・標準出力・標準エラーはそのまま引き継ぐ。正常終了とパニックの終了コードは、生成プログラムの値をそのまま返す。

**[tool.run-signal]** 対応するUnix環境では、生成プログラムがシグナルで終了した場合、後処理の後に `suz run` 自身も同じシグナルで終了する。端末からの割り込みは生成プログラムにも届くようにする。

```sh
suz run program.suz
```

### 2.3 check

**[tool.check]** 入力に対して字句・構文・名前解決・型・定数評価・制御・診断設定の検査を行う。成功時は終了コード0を返す。このコマンドはClangやSDKの用意に依存せず使える。

```sh
suz check program.suz
```

### 2.4 emit-c

**[tool.emit-c]** Cの調査や環境層との組み込みに使う一式を `DIRECTORY` に出力する。出力内容は次のとおりである。

| パス | 内容 |
|---|---|
| `program.c` | 入力から生成したC |
| `LICENSE-MIT`・`LICENSE-APACHE` | Suzのライセンス |
| `runtime/` | 同じSuzバイナリに同梱されたランタイム・環境層・Ryu・ライセンス |
| `scripts/build-c.sh` | 数値設定を固定して、この一式から実行ファイルを作るコマンド |

```sh
suz emit-c program.suz -o generated
cd generated
./scripts/build-c.sh program.c -o program -O2
```

## 3. オプション

**[tool.options]** コマンドごとに次のオプションを受け付ける。

| オプション | build | run | check | emit-c | 意味 |
|---|---|---|---|---|---|
| `-o PATH` | 必須 | | | 必須 | 出力先 |
| `-O0`、`-O2` | 可 | 可 | | | 最適化。既定は `-O0` |
| `--cc PATH` | 可 | 可 | | | Clangの実行ファイル |
| `--env FILE` | 可 | 可 | | 可 | 標準環境層を置き換えるCソース |
| `--text-pool-limit SIZE` | 可 | 可 | | 可 | テキストプールの上限。既定は `1G` |
| `--force` | 可 | | | | 既存の通常ファイルを出力先として置き換える |
| `--warn-unhandled-arithmetic` | 可 | 可 | 可 | 可 | 整数演算の失敗処理がない箇所を警告する |
| `--deny-unhandled-arithmetic` | 可 | 可 | 可 | 可 | 同じ箇所をコンパイルエラーにする |
| `--help` | 可 | 可 | 可 | 可 | そのコマンドの使用方法を表示する |

`-O0` と `-O2`、整数演算の警告と強制は、それぞれ排他的である。診断設定の意味は言語仕様の `[diagnostic.arithmetic]` に従う。設定を省略した場合は通常設定である。

**[tool.environment]** `--env` のCソースは、ランタイム仕様の環境関数を実装する。`build` と `run` はそのファイルを標準環境層の代わりにリンクする。`emit-c` はその内容を出力先の `runtime/suz_env.c` に収録する。環境層のコンパイルには同梱の `suz_env.h` を使う。

**[tool.build-settings]** C11と厳密な浮動小数点演算を使い、積和の縮約を停止し、非正規化数をIEEEの規則で扱う。Clangへの数値関連オプションはSuzが固定する。具体的には `-ffp-model=strict`、`-ffp-contract=off`、`-fdenormal-fp-math=ieee` を指定する。`CFLAGS`、`CPPFLAGS`、`LDFLAGS` に空でない値がある場合、`build` と `run` は使用方法のエラーにする。

### 3.1 テキストプールの上限

**[tool.text-pool-limit]** `--text-pool-limit SIZE` は、生成するプログラムのテキストプールの上限を指定する。`build`、`run`、`emit-c` で受け付ける。省略時は `1G` とする。設定は生成するプログラムに組み込む。`emit-c` では設定を生成物に含め、そのCから作った実行ファイルにも適用する。

| SIZE | 意味 |
|---|---|
| 正の10進整数 | バイト単位の上限 |
| 正の10進整数に `K`、`M`、`G` を付けた値 | それぞれ1,024、1,048,576、1,073,741,824倍のバイト数 |
| `unlimited` | 無制限 |

1Gは1 GiB、すなわち1,073,741,824バイトである。無制限はプール固有の容量制限を設けないという意味であり、実行環境のメモリや長さの表現範囲には制約される。計測対象はランタイム仕様の [runtime.text-accounting] に従う。

**[tool.text-pool-size]** 数値部分はASCIIの数字だけとし、単位は大文字で書く。0、負数、不正な表記、単位換算後の値が64ビット符号なし整数の範囲を超える指定、同じオプションの重複指定は使用方法のエラーとする。単位換算はオーバーフローを検査してから結果を使う。

```sh
suz build program.suz -o program --text-pool-limit 256M
suz run program.suz --text-pool-limit 2G
suz emit-c program.suz -o generated --text-pool-limit unlimited
```

## 4. 出力と一時ファイル

**[tool.output]** `build` の既存出力は、`--force` がある場合にだけ置き換える。ディレクトリとシンボリックリンクは置き換え対象にできない。入力ソース、指定した環境層と同じファイルを出力先に指定すると、`--force` があってもエラーである。同一性の判定には、別名やハードリンクによる参照も含める。`emit-c` の出力先には新規ディレクトリを指定する。

**[tool.publish-output]** 成果物を完成させてから最終出力先に反映する。言語の検査、Cのコンパイル・リンク、成果物の準備に失敗した場合は、既存の出力を保持する。`--force` を省略した場合、処理中に別のプロセスが出力先を作成しても上書きしない。

**[tool.temporary]** C、同梱ソースの展開物、中間成果物、`run` の実行ファイルには、呼び出しごとに独立した一時領域を使う。正常終了と処理可能なエラーでは一時領域を削除する。`run` では子プロセスの終了後に削除する。一時領域の名前を診断のSuzソース名として使わず、利用者が指定した入力パスを保持する。

## 5. メッセージと終了

**[tool.messages]** ツールのメッセージは英語で出力する。`build`、`check`、`emit-c` の成功時は標準出力を空にする。警告・エラーは標準エラーに出力する。言語の診断は次の形式とし、規則名は言語仕様から使う。

```text
FILE:LINE:COLUMN: MESSAGE [rule]
FILE:LINE:COLUMN: warning: MESSAGE [rule]
```

ツールの使用方法・入出力・外部ツールのエラーは `suz: MESSAGE [tool.rule]` の形式にする。Clangが出力した診断は標準エラーへ伝え、Suzの言語診断と区別する。

**[tool.exit]** ツールの終了コードは次のとおりである。

| コード | 意味 |
|---|---|
| 0 | コマンドの成功 |
| 1 | Suzソースのコンパイルエラー |
| 2 | 引数・ファイル入出力・外部ツールやSDKの不足・外部ツールの起動・子プロセスの起動や待機のエラー |
| 3 | Suzの内部エラー |
| 4 | 起動したClangのコンパイル・リンクが失敗した場合 |

`run` で生成プログラムを起動できた場合は `[tool.run]` と `[tool.run-signal]` に従う。プログラムの終了コードとツールのエラーコードは値が重なるため、ツール側の失敗は診断も併せて示す。

**[tool.information]** `--version` は `suz v0.3.1` の形式の1行を標準出力に表示する。`--help` と各コマンドの `--help` は、使用方法・オプション・既定値を標準出力に表示する。`--licenses` は、Suzと同梱物の著作権・ライセンス表示を標準出力に表示する。これらは入力ファイルとClangを用意せず実行でき、成功時は終了コード0である。
