#ifndef SUZ_PARSE_H
#define SUZ_PARSE_H

#include "suz_char.h"
#include <stdbool.h>
#include <stdint.h>

/* バイト列全体を、メモリ確保とロケールへの依存なしで解析する。
   bitsは8・16・32・64。modeは0=検査あり、1=ラップ、2=飽和。整数の結果は2の補数のビット列で、成功時だけ出力を書き込む。 */
bool suz_parse_integer(const char *bytes, uint64_t length, unsigned bits,
                       bool signed_target, uint8_t mode, uint64_t *output);
bool suz_parse_bool(const char *bytes, uint64_t length, bool *output);

/* Suzの十進表記からIEEEのビット列へ、最近接偶数丸めで直接変換する。
   widthは32または64。有限値のオーバーフローは失敗、非正規化数と符号付きゼロは成功とする。
   解析には固定の作業領域を使い、メモリ確保をしない。 */
bool suz_parse_float_bits(const char *bytes, uint64_t length, unsigned width, uint64_t *output);
bool suz_parse_f32(const char *bytes, uint64_t length, float *output);
bool suz_parse_f64(const char *bytes, uint64_t length, double *output);

/* 引用付きの文字列値を一つ読む。検証だけなら出力先はNULLでよく、出力するなら検証時のdecoded_lengthバイト以上必要。
   消費長には両端のバッククォートを含む。復号先は入力の先頭と重ねられる。失敗時の出力は使わない。 */
bool suz_parse_string(const char *bytes, uint64_t length, uint64_t *consumed,
                       char *output, uint64_t *decoded_length);

/* 構造体のtext確保用に、検証と同時にUnicodeスカラー値の個数も数える。 */
bool suz_parse_string_counts(const char *bytes, uint64_t length, uint64_t *consumed,
                            char *output, uint64_t *decoded_length, uint64_t *scalars);

/* 文字列全体からスカラー値を一つ、または先頭から引用付きリテラルを一つ解析する。
   失敗時の出力は使わない。どちらもメモリ確保をしない。 */
bool suz_parse_char(const char *bytes, uint64_t length, uint32_t *output);
enum suz_char_literal_status {
    SUZ_CHAR_LITERAL_OK, SUZ_CHAR_LITERAL_SYNTAX, SUZ_CHAR_LITERAL_EMPTY_ESCAPE,
    SUZ_CHAR_LITERAL_TOO_MANY_DIGITS, SUZ_CHAR_LITERAL_SURROGATE,
    SUZ_CHAR_LITERAL_OUT_OF_RANGE
};
/* 同じパーサーで、コンパイラの診断用に状態コードを返す。 */
uint32_t suz_parse_char_literal_status(const char *bytes, uint64_t length,
                                       uint32_t *output, uint64_t *consumed);
bool suz_parse_char_literal(const char *bytes, uint64_t length,
                             uint32_t *output, uint64_t *consumed);
#endif
