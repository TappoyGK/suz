#ifndef SUZ_ARENA_H
#define SUZ_ARENA_H

#include "suz_config.h"
#include <stdckdint.h>
#include <stddef.h>
#include <stdint.h>

/* コンパイラ内部のAPI。使用位置の保存・復帰は生成Cでusedを読み書きする。
   復帰先は現在の使用位置以下の、対応する領域進入時の値でなければならない。
   生存する参照の検査はコンパイラが行い、ランタイムは参照をたどらない。 */
typedef struct {
    unsigned char *bytes;
    size_t capacity;
    size_t used;
} suz_arena_state;
extern suz_arena_state suz_arena;

/* arenaを使うプログラムだけが、Suzのmainより前に1回呼ぶ。
   capacityは1以上。環境層から確保した領域はプロセス終了まで保持する。 */
void suz_arena_init(uint64_t capacity, const char *source, size_t line, size_t column);

/* 診断は失敗時だけ別の翻訳単位で実行し、通常の確保に呼び出しを残さない。 */
_Noreturn __attribute__((cold)) void suz_arena_overflow(const char *source, size_t line, size_t column);
_Noreturn __attribute__((cold)) void suz_arena_capacity_exceeded(size_t additional, const char *source, size_t line, size_t column);

/* 生成Cは初期化後に、size > 0と型のalignof（2のべき乗、max_align_t以下）を渡す。
   初期値は呼び出し前に評価済み。常時インライン化し、整列とサイズを定数として最適化できるようにする。 */
static inline __attribute__((always_inline)) void *suz_arena_alloc(size_t size, size_t alignment, const char *source, size_t line, size_t column) {
    /* 基底アドレスはmax_align_tに整列済み。符号なしの剰余算術で余白を求め、
       最適化なしでも割り算を行わない。size_tの加算のあふれは後続の検査で拒否する。 */
    size_t padding = (-suz_arena.used) & (alignment - 1);
    size_t additional, end;
    if (__builtin_expect(ckd_add(&additional, padding, size) || ckd_add(&end, suz_arena.used, additional), 0)) {
        suz_arena_overflow(source, line, column);
    }
    if (__builtin_expect(end > suz_arena.capacity, 0)) {
        suz_arena_capacity_exceeded(additional, source, line, column);
    }
    /* 検査が完了するまで使用位置を変えず、範囲外のポインタも作らない。 */
    void *result = suz_arena.bytes + suz_arena.used + padding;
    suz_arena.used = end;
    return result;
}

/* Clangのcleanupは通常終了・return・break・continue・外向きgotoで働く。
   戻り値を評価してから回収し、パニックによるプロセス終了には回収を要求しない。
   最適化なしでも復帰だけの関数呼び出しを残さない。 */
static inline __attribute__((always_inline)) void suz_arena_restore(size_t *mark) {
    suz_arena.used = *mark;
}

#endif
