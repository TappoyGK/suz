#ifndef SUZ_RUNTIME_H
#define SUZ_RUNTIME_H

#include "suz_config.h"
#include "suz_env.h"
#include "suz_char.h"

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* 内部の正確な整数。成功する全演算が、許可する符号付き・符号なし128ビット範囲に収まることは値域解析で保証する。 */
typedef struct {
    unsigned _BitInt(128) magnitude;
    bool negative;
    bool failed;
    size_t line;
    size_t column;
} suz_exact;
static inline suz_exact suz_exact_make(bool negative, unsigned _BitInt(128) magnitude) {
    return (suz_exact){magnitude, negative && magnitude != 0, false, 0, 0};
}
static inline suz_exact suz_exact_i64(int64_t n) {
    return suz_exact_make(n < 0, n < 0 ? UINT64_C(0) - (uint64_t)n : (uint64_t)n);
}
static inline suz_exact suz_exact_neg(suz_exact a) {
    a.negative = !a.negative && a.magnitude != 0;
    return a;
}
static inline int suz_exact_compare(suz_exact a, suz_exact b) {
    if (a.negative != b.negative) return a.negative ? -1 : 1;
    int order = (a.magnitude > b.magnitude) - (a.magnitude < b.magnitude);
    return a.negative ? -order : order;
}
static inline suz_exact suz_exact_add(suz_exact a, suz_exact b) {
    if (a.negative == b.negative) return suz_exact_make(a.negative, a.magnitude + b.magnitude);
    if (a.magnitude >= b.magnitude) return suz_exact_make(a.negative, a.magnitude - b.magnitude);
    return suz_exact_make(b.negative, b.magnitude - a.magnitude);
}
static inline suz_exact suz_exact_sub(suz_exact a, suz_exact b) {
    return suz_exact_add(a, suz_exact_neg(b));
}
static inline suz_exact suz_exact_mul(suz_exact a, suz_exact b) {
    return suz_exact_make(a.negative != b.negative, a.magnitude * b.magnitude);
}
static inline suz_exact suz_exact_divrem(suz_exact a, suz_exact b, bool remainder, size_t line, size_t column) {
    if (b.magnitude == 0) return (suz_exact){0, false, true, line, column};
    return suz_exact_make(remainder ? a.negative : a.negative != b.negative,
                         remainder ? a.magnitude % b.magnitude : a.magnitude / b.magnitude);
}
static inline suz_exact suz_exact_euclid(suz_exact a, suz_exact b, size_t line, size_t column) {
    if (b.magnitude == 0) return (suz_exact){0, false, true, line, column};
    unsigned _BitInt(128) r = a.magnitude % b.magnitude;
    if (a.negative && r != 0) r = b.magnitude - r;
    return suz_exact_make(false, r);
}
static inline bool suz_exact_fits(suz_exact a, bool is_signed, unsigned bits) {
    if (a.failed) return false;
    if (a.negative) return is_signed && a.magnitude <= ((unsigned _BitInt(128))1 << (bits - 1));
    return a.magnitude <= (((unsigned _BitInt(128))1 << (bits - (is_signed ? 1 : 0))) - 1);
}
static inline uint64_t suz_exact_bits(suz_exact a) {
    return a.negative ? UINT64_C(0) - (uint64_t)a.magnitude : (uint64_t)a.magnitude;
}
static inline int64_t suz_exact_signed(suz_exact a) {
    if (!a.negative) return (int64_t)a.magnitude;
    return -(int64_t)(a.magnitude - 1) - 1;
}
static inline double suz_exact_f64(suz_exact a) {
    double n = (double)a.magnitude;
    return a.negative ? -n : n;
}
static inline float suz_exact_f32(suz_exact a) {
    float n = (float)a.magnitude;
    return a.negative ? -n : n;
}
/* 整数を丸めずに数学的な値を比較する。-1・0・1、または2（順序付け不可）を返す。 */
int suz_exact_compare_float(suz_exact a, double b);
size_t suz_format_exact(suz_exact a, char output[40]);
void suz_log_exact(suz_exact a);

/* Suzの関数を呼ぶ前に、トラップなし・最近接丸めの標準の浮動小数点環境を設定する。
   ホストで設定できなければ異常終了する。 */
void suz_runtime_init(void);

/* 対象のSuz整数型の最小値・最大値を渡す（符号なしの最小値は0）。
   両オペランドはその範囲内で、resultは書き込み可能でなければならない。成功時は*resultを書いてtrueを返す。
   オーバーフローとゼロ除算では*resultを変えずfalseを返す。符号付き最小値 / -1は除算・剰余とも失敗する。
   除算と剰余は失敗条件を確認してから評価する。加減乗算はC23の検査付き演算を使い、その後で対象の値域を確認する。 */
bool suz_try_add_i64(int64_t a, int64_t b, int64_t min, int64_t max, int64_t *result);
bool suz_try_sub_i64(int64_t a, int64_t b, int64_t min, int64_t max, int64_t *result);
bool suz_try_mul_i64(int64_t a, int64_t b, int64_t min, int64_t max, int64_t *result);
bool suz_try_div_i64(int64_t a, int64_t b, int64_t min, int64_t max, int64_t *result);
bool suz_try_rem_i64(int64_t a, int64_t b, int64_t min, int64_t max, int64_t *result);
bool suz_try_add_u64(uint64_t a, uint64_t b, uint64_t max, uint64_t *result);
bool suz_try_sub_u64(uint64_t a, uint64_t b, uint64_t max, uint64_t *result);
bool suz_try_mul_u64(uint64_t a, uint64_t b, uint64_t max, uint64_t *result);
bool suz_try_div_u64(uint64_t a, uint64_t b, uint64_t max, uint64_t *result);
bool suz_try_rem_u64(uint64_t a, uint64_t b, uint64_t max, uint64_t *result);

/* widthは8・16・32・64で、aはその符号付きまたは符号なし型に収まる必要がある。
   シフト量は呼び出す前にuint64_tへ変換する。負の量はwidthを超え、他の範囲外の量と同じ検査で失敗する。
   count >= widthまたは左シフトのオーバーフローでは*resultを変えずfalseを返す。
   それ以外は結果を書いてtrueを返す。resultは書き込み可能でなければならない。 */
bool suz_try_shl_i64(int64_t a, uint64_t count, unsigned width, int64_t *result);
bool suz_try_shr_i64(int64_t a, uint64_t count, unsigned width, int64_t *result);
bool suz_try_lshr_i64(int64_t a, uint64_t count, unsigned width, int64_t *result);
bool suz_try_shl_u64(uint64_t a, uint64_t count, unsigned width, uint64_t *result);
bool suz_try_shr_u64(uint64_t a, uint64_t count, unsigned width, uint64_t *result);

// panic文は開始後にsuz_log_*で評価済みの断片を出力し、終了時までロックを保持する。
void suz_panic_begin(const char *source, size_t line, size_t column);
_Noreturn void suz_panic_end(void);
_Noreturn void suz_panic(const char *source, size_t line, size_t column, const char *reason);

static inline suz_exact suz_exact_require(suz_exact a, const char *source) {
    if (a.failed) suz_panic(source, a.line, a.column, "integer division by zero [rt.div-zero]");
    return a;
}

/* beginの前にすべての補間値を評価する。beginとendは対にし、書き込み処理には評価済みの値だけを渡す。 */
/* sourceがNULLなら位置接頭辞を出さず、同じ出力ロックだけを取得する。 */
void suz_log_begin(suz_log_level level, const char *source, size_t line);
void suz_log_text(const char *text, size_t len);
void suz_log_i64(int64_t value);
void suz_log_u64(uint64_t value);
void suz_log_bool(bool value);
static inline void suz_log_char(uint32_t value) {
    char bytes[4];
    size_t length = suz_char_encode(value, bytes);
    suz_log_text(bytes, length);
}
/* 出力には32バイト必要。終端のNULを含めない長さを返す。 */
size_t suz_format_f32(float value, char output[32]);
size_t suz_format_f64(double value, char output[32]);
void suz_log_f32(float value);
void suz_log_f64(double value);
void suz_log_end(void);

#endif
