# Ryu

浮動小数点ログの最短表記への変換に、[ulfjack/ryu](https://github.com/ulfjack/ryu) のC実装を使う。

- 固定リビジョン：`4c0618b0e44f7ef027ebae05d2cc7812048f7c8f`
- 取得元：<https://github.com/ulfjack/ryu/tree/4c0618b0e44f7ef027ebae05d2cc7812048f7c8f>
- 同梱範囲：`f2s.c`、`d2s.c`と、それらが参照するヘッダー・変換表
- ライセンス：[Boost Software License 1.0](LICENSE-Boost)を選択する。上流が併記する[Apache License 2.0](LICENSE-Apache2)も同梱する。

上流のファイルは変更せずに収録する。Suzはメモリ確保を行わない `f2s_buffered_n` と `d2s_buffered_n` を使う。小数点、指数表記への切り替え、特殊値の表示は [suz_float.c](../../suz_float.c) で整える。

更新時は上記リビジョンとソースを一緒に更新し、`cargo test --test spec_cases float_logs::` と例集の実行テストで検証する。
