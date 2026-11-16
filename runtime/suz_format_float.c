#include "suz_runtime.h"
#include "ryu/ryu.h"

#include <float.h>
#include <string.h>

static bool exponential(double value) {
    double magnitude = value < 0 ? -value : value;
    /* 10^-6は、このbinary64値と次の値の間にある。
       丸めた1e-6リテラルとの厳密な大小比較ではなく、下側の隣接値を含めて比較する。
       binary32からbinary64への拡大は正確である。 */
    return magnitude >= 1e21 ||
           (magnitude > 0 && magnitude <= 0x1.0c6f7a0b5ed8dp-20);
}

/* Ryuは最大17桁の有効数字をd.ddE[-]ddd形式で返す。
   Suzの固定表記は整数部最大21桁、または小数部先頭のゼロ最大6桁を必要とする。
   どちらの形式も符号と小数点を含めて32バイトに収まる。 */
static size_t format_float(const char *shortest, int length, bool scientific, char output[32]) {
    size_t used = 0;
    int cursor = 0;
    if (shortest[cursor] == '-') {
        output[used++] = '-';
        ++cursor;
    }
    if (shortest[cursor] == 'N') {
        memcpy(output, "nan", 3);
        return 3;
    }
    if (shortest[cursor] == 'I') {
        memcpy(output + used, "inf", 3);
        return used + 3;
    }
    char digits[17];
    int count = 0;
    while (shortest[cursor] != 'E') {
        if (shortest[cursor] != '.') digits[count++] = shortest[cursor];
        ++cursor;
    }
    int exponent_start = ++cursor;
    bool negative = shortest[cursor] == '-';
    if (negative) ++cursor;
    int exponent = 0;
    while (cursor < length) exponent = exponent * 10 + shortest[cursor++] - '0';
    if (negative) exponent = -exponent;
    if (scientific) {
        output[used++] = digits[0];
        output[used++] = '.';
        if (count == 1) output[used++] = '0';
        for (int i = 1; i < count; ++i) output[used++] = digits[i];
        output[used++] = 'e';
        while (exponent_start < length) output[used++] = shortest[exponent_start++];
    } else {
        int point = exponent + 1;
        if (point <= 0) {
            output[used++] = '0';
            output[used++] = '.';
            for (int i = 0; i < -point; ++i) output[used++] = '0';
            for (int i = 0; i < count; ++i) output[used++] = digits[i];
        } else {
            for (int i = 0; i < count || i < point; ++i) {
                if (i == point) output[used++] = '.';
                output[used++] = i < count ? digits[i] : '0';
            }
            if (count <= point) {
                output[used++] = '.';
                output[used++] = '0';
            }
        }
    }
    return used;
}

size_t suz_format_f32(float value, char output[32]) {
    char shortest[32];
    int length = f2s_buffered_n(value, shortest);
    return format_float(shortest, length, exponential(value), output);
}

size_t suz_format_f64(double value, char output[32]) {
    char shortest[32];
    int length = d2s_buffered_n(value, shortest);
    return format_float(shortest, length, exponential(value), output);
}

