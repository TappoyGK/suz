#ifndef SUZ_ARENA_H
#define SUZ_ARENA_H

#include "suz_config.h"
#include <stdckdint.h>
#include <stddef.h>
#include <stdint.h>

#if defined(__has_feature)
#if __has_feature(address_sanitizer)
#define SUZ_ARENA_ASAN 1
#endif
#endif
#if defined(__SANITIZE_ADDRESS__) && !defined(SUZ_ARENA_ASAN)
#define SUZ_ARENA_ASAN 1
#endif
#ifdef SUZ_ARENA_ASAN
#include <sanitizer/asan_interface.h>
/* 対応環境のASanは8バイトを1単位として管理する。実体同士で単位を共有しない。 */
#define SUZ_ARENA_ASAN_UNIT 8
static_assert(alignof(max_align_t) >= SUZ_ARENA_ASAN_UNIT);
#endif

/* ブロックを追加しても既存の実体を移動しない。currentより後ろは回収済みで再利用できる。
   容量の上限は管理情報を除く全ブロックの合計に適用し、整列の余白も容量を消費する。 */
typedef struct suz_arena_chunk {
    struct suz_arena_chunk *next;
    size_t capacity;
    alignas(max_align_t) unsigned char bytes[];
} suz_arena_chunk;
typedef struct {
    unsigned char *bytes;
    size_t capacity;
    size_t used;
    suz_arena_chunk *current;
} suz_arena_cursor;
/* 生成Cは1つの翻訳単位。確保位置のアドレスを外部へ渡さず、拡張関数には値で渡す。
   外部呼出しでこの状態が書き換わらないため、通常経路ではレジスタに保持できる。 */
static suz_arena_cursor suz_arena_position;
typedef struct {
    suz_arena_chunk *first;
    uint64_t total;
    uint64_t grow;
    uint64_t limit; /* 0は無制限。 */
} suz_arena_state;
extern suz_arena_state suz_arena;

#if defined(SUZ_ARENA_GROW) && SUZ_ARENA_GROW == 0
/* 拡張禁止の生成Cではブロックが変わらないため、使用位置だけを保存する。 */
typedef size_t suz_arena_mark;
#else
typedef struct {
    suz_arena_chunk *chunk;
    size_t used;
} suz_arena_mark;
#endif

/* arenaを使うプログラムだけがmainの前に呼ぶ。initial=0なら最初の要求まで確保しない。
   grow=0は追加確保禁止。初期容量は有限のlimitを超えないように抑える。 */
void suz_arena_destroy(void);
_Noreturn __attribute__((cold)) void suz_array_bounds(const char *source, size_t line, size_t column, size_t dimension, uint64_t index, bool negative, int32_t length);
_Noreturn __attribute__((cold)) void suz_array_slice_bounds(const char *source, size_t line, size_t column, uint64_t start, bool start_negative, uint64_t end, bool end_negative, int32_t length);
_Noreturn __attribute__((cold)) void suz_array_dimensions(const char *source, size_t line, size_t column, const char *name, int32_t expected, int32_t actual);
suz_arena_cursor suz_arena_storage_init(uint64_t initial, uint64_t grow, uint64_t limit, const char *source, size_t line, size_t column);
_Noreturn __attribute__((cold)) void suz_arena_overflow(const char *source, size_t line, size_t column);
_Noreturn __attribute__((cold)) void suz_arena_capacity_exceeded(size_t size, const char *source, size_t line, size_t column);
/* 現在のブロックに入らない実体は、回収済みのブロックか新しいブロックの先頭に置く。 */
suz_arena_cursor suz_arena_expand(suz_arena_chunk *current, size_t size, const char *source, size_t line, size_t column);
#ifdef SUZ_ARENA_ASAN
/* 保存位置以後の実体を無効化する。ブロックの管理情報には触れない。 */
void suz_arena_poison_reclaimed(suz_arena_chunk *saved, size_t used, suz_arena_cursor current);
#endif

static inline void suz_arena_init(uint64_t initial, uint64_t grow, uint64_t limit, const char *source, size_t line, size_t column) {
    suz_arena_position = suz_arena_storage_init(initial, grow, limit, source, line, column);
}

/* 初期値は呼出し前に評価済み。size>0、alignmentは2のべき乗でmax_align_t以下。 */
static inline __attribute__((always_inline)) void *suz_arena_alloc(size_t size, size_t alignment, const char *source, size_t line, size_t column) {
#ifdef SUZ_ARENA_ASAN
    size_t object_size = size;
    if (ckd_add(&size, size, SUZ_ARENA_ASAN_UNIT - 1)) suz_arena_overflow(source, line, column);
    size &= ~(size_t)(SUZ_ARENA_ASAN_UNIT - 1);
    if (alignment < SUZ_ARENA_ASAN_UNIT) alignment = SUZ_ARENA_ASAN_UNIT;
#endif
    size_t padding = (-suz_arena_position.used) & (alignment - 1);
    size_t additional, end;
#if defined(SUZ_ARENA_GROW) && SUZ_ARENA_GROW == 0
    /* 設定が固定なら、失敗経路から戻らないことまでCコンパイラへ伝える。
       確保ごとの外部呼出しやブロックの再読込みを通常経路に残さない。 */
    if (__builtin_expect(ckd_add(&additional, padding, size) || ckd_add(&end, suz_arena_position.used, additional), 0)) {
        suz_arena_overflow(source, line, column);
    }
    if (__builtin_expect(end > suz_arena_position.capacity, 0)) {
        suz_arena_capacity_exceeded(size, source, line, column);
    }
#else
    /* 現在のブロック内の加算があふれても、新しいブロックなら収まる場合がある。 */
    if (__builtin_expect(ckd_add(&additional, padding, size) || ckd_add(&end, suz_arena_position.used, additional) || end > suz_arena_position.capacity, 0)) {
        suz_arena_position = suz_arena_expand(suz_arena_position.current, size, source, line, column);
#ifdef SUZ_ARENA_ASAN
        __asan_unpoison_memory_region(suz_arena_position.bytes, object_size);
#endif
        return suz_arena_position.bytes;
    }
#endif
    void *result = suz_arena_position.bytes + suz_arena_position.used + padding;
    suz_arena_position.used = end;
#ifdef SUZ_ARENA_ASAN
    __asan_unpoison_memory_region(result, object_size);
#endif
    return result;
}

static inline __attribute__((always_inline)) suz_arena_mark suz_arena_save(void) {
#if defined(SUZ_ARENA_GROW) && SUZ_ARENA_GROW == 0
    return suz_arena_position.used;
#else
    return (suz_arena_mark){suz_arena_position.current, suz_arena_position.used};
#endif
}

/* cleanupは通常終了・return・break・continue・外向きgotoに適用する。
   内容の消去や解放は行わず、保存したブロックと使用位置へ戻す。 */
static inline __attribute__((always_inline)) void suz_arena_restore(suz_arena_mark *mark) {
#if defined(SUZ_ARENA_GROW) && SUZ_ARENA_GROW == 0
#ifdef SUZ_ARENA_ASAN
    suz_arena_poison_reclaimed(suz_arena_position.current, *mark, suz_arena_position);
#endif
    suz_arena_position.used = *mark;
#else
#ifdef SUZ_ARENA_ASAN
    suz_arena_poison_reclaimed(mark->chunk, mark->used, suz_arena_position);
#endif
    if (__builtin_expect(suz_arena_position.current != mark->chunk, 0)) {
        suz_arena_position.current = mark->chunk;
        suz_arena_position.bytes = mark->chunk ? mark->chunk->bytes : NULL;
        suz_arena_position.capacity = mark->chunk ? mark->chunk->capacity : 0;
    }
    suz_arena_position.used = mark->used;
#endif
}

#endif
