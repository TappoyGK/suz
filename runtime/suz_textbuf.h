#ifndef SUZ_TEXTBUF_H
#define SUZ_TEXTBUF_H

#include "suz_arena.h"
#include "suz_text.h"
#include "suz_parse.h"
#include <string.h>

_Noreturn void suz_textbuf_length_failure(uint64_t scalars, suz_text_location location);

/* 現在の作業領域のカーソルを使うため、生成C側で展開する。
   空文字列にも実体を置き、静的な空文字列と区別する。 */
static inline suz_text_view suz_textbuf_allocate(uint64_t length, uint64_t scalars,
                                               uint64_t capacity, suz_text_location location) {
    if (scalars > INT32_MAX) suz_textbuf_length_failure(scalars, location);
    if (capacity < length) capacity = length;
    size_t size;
    if (capacity > SIZE_MAX || ckd_add(&size, sizeof(suz_text_storage), (size_t)capacity)) {
        suz_arena_overflow(location.source, location.line, location.column);
    }
    suz_text_storage *storage = suz_arena_alloc(size, alignof(suz_text_storage),
                                               location.source, location.line, location.column);
    storage->capacity = capacity;
    storage->used = length;
    return (suz_text_view){storage->bytes, length, scalars, storage};
}

/* 断片は評価済みの借用ビュー。完成した中間文字列を作らず、1回の確保へ直接コピーする。 */
static inline suz_text_view suz_textbuf_build(uint64_t capacity, const suz_text_view *parts,
                                            size_t count, suz_text_location location) {
    uint64_t length = 0, scalars = 0;
    for (size_t i = 0; i < count; ++i) {
        if (ckd_add(&length, length, parts[i].length)) {
            suz_arena_overflow(location.source, location.line, location.column);
        }
        scalars += parts[i].scalars;
        if (scalars > INT32_MAX) suz_textbuf_length_failure(scalars, location);
    }
    suz_text_view result = suz_textbuf_allocate(length, scalars, capacity, location);
    char *out = result.storage->bytes;
    for (size_t i = 0; i < count; ++i) {
        if (parts[i].length) memcpy(out, parts[i].bytes, (size_t)parts[i].length);
        out += parts[i].length;
    }
    return result;
}

/* 左辺の旧範囲は変えず、使用済み末尾より後ろだけを書き換える。
   右辺が同じ実体のビューでも、その範囲は旧末尾までなので書き込み先と重ならない。 */
static inline suz_text_view suz_textbuf_append(suz_text_view left, const suz_text_view *parts,
                                             size_t count, suz_text_location location) {
    uint64_t length = left.length, scalars = left.scalars;
    for (size_t i = 0; i < count; ++i) {
        if (ckd_add(&length, length, parts[i].length)) {
            suz_arena_overflow(location.source, location.line, location.column);
        }
        scalars += parts[i].scalars;
        if (scalars > INT32_MAX) suz_textbuf_length_failure(scalars, location);
    }
    // 現在の作業領域に属する実体があることはコンパイラが検査する。
    suz_text_storage *storage = left.storage;
    uint64_t end = (uint64_t)(left.bytes - storage->bytes) + left.length;
    suz_text_view result = left;
    if (end != storage->used || length - left.length > storage->capacity - end) {
        uint64_t capacity = storage->capacity ? storage->capacity : 16;
        while (capacity < length) {
            if (ckd_mul(&capacity, capacity, UINT64_C(2))) {
                suz_arena_overflow(location.source, location.line, location.column);
            }
        }
        result = suz_textbuf_allocate(length, scalars, capacity, location);
        if (left.length) memcpy(result.storage->bytes, left.bytes, (size_t)left.length);
    } else {
        result.storage->used += length - left.length;
        result.length = length;
        result.scalars = scalars;
    }
    char *out = (char *)result.bytes + left.length;
    for (size_t i = 0; i < count; ++i) {
        if (parts[i].length) memcpy(out, parts[i].bytes, (size_t)parts[i].length);
        out += parts[i].length;
    }
    return result;
}

/* 構造体全体の検証後に呼ぶ。検証時の復号長と個数を使い、保存先へ直接復号する。 */
static inline suz_text_view suz_textbuf_from_parsed(const char *bytes, uint64_t length,
                                                 uint64_t decoded, uint64_t scalars,
                                                 suz_text_location location) {
    suz_text_view result = suz_textbuf_allocate(decoded, scalars, decoded, location);
    uint64_t consumed, written;
    (void)suz_parse_string(bytes, length, &consumed, result.storage->bytes, &written);
    return result;
}
#endif
