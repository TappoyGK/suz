#include "suz_runtime.h"
#include "suz_env.h"
#include "ryu/ryu.h"

#include <float.h>
#include <string.h>

static bool exponential(double value) {
    double magnitude = value < 0 ? -value : value;
    /* 10^-6 lies strictly between this binary64 value and its successor.
       Compare to the lower neighbor inclusively, not to a rounded 1e-6
       literal exclusively. Promotion from binary32 to binary64 is exact. */
    return magnitude >= 1e21 ||
           (magnitude > 0 && magnitude <= 0x1.0c6f7a0b5ed8dp-20);
}

/* Ryu supplies at most 17 significant digits, in d.ddE[-]ddd form.
   Suz's fixed form needs at most 21 integer digits or 6 leading fractional
   zeroes. Both forms, including a sign and decimal point, fit in 32 bytes. */
static void write_float(const char *shortest, int length, bool scientific) {
    char output[32];
    size_t used = 0;
    int cursor = 0;
    if (shortest[cursor] == '-') {
        output[used++] = '-';
        ++cursor;
    }
    if (shortest[cursor] == 'N') {
        suz_env_log("nan", 3);
        return;
    }
    if (shortest[cursor] == 'I') {
        memcpy(output + used, "inf", 3);
        suz_env_log(output, used + 3);
        return;
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
    suz_env_log(output, used);
}

void suz_log_f32(float value) {
    char shortest[32];
    int length = f2s_buffered_n(value, shortest);
    write_float(shortest, length, exponential(value));
}

void suz_log_f64(double value) {
    char shortest[32];
    int length = d2s_buffered_n(value, shortest);
    write_float(shortest, length, exponential(value));
}
