#ifndef SUZ_ARENA_REPORT_H
#define SUZ_ARENA_REPORT_H
#include <stddef.h>
#include <stdint.h>

typedef enum {
    SUZ_ARENA_REPORT_TREE = 1,
    SUZ_ARENA_REPORT_TREE_BYTES = 2,
    SUZ_ARENA_REPORT_CSV = 3,
} suz_arena_report_format;

/* 表示の行は繰り返し・再帰を集約する。実際の進入履歴は別のスタック上のframeで持つ。 */
typedef struct {
    const char *source;
    const char *function;
    size_t site_id, line, column;
    size_t parent, first_child, last_child, next_sibling, depth;
    size_t peak;
    bool recursive, omitted;
} suz_arena_report_node;

typedef struct suz_arena_report_frame suz_arena_report_frame;
/* 各ソース位置に1つだけ生成する。再帰の外側の進入と、直前に使った親の行を保持する。 */
typedef struct {
    size_t id;
    const char *source, *function;
    size_t line, column;
    size_t cached_parent, cached_node;
    suz_arena_report_frame *active;
} suz_arena_report_site;

struct suz_arena_report_frame {
    struct suz_arena_report_frame *previous;
    size_t node, base, peak;
    suz_arena_report_site *site;
    bool outermost;
};

typedef struct {
    suz_arena_report_node *nodes;
    size_t *path; /* 出力用の祖先列。nodesと同じ一度の確保に含める。 */
    size_t count, capacity;
    size_t *lookup, lookup_capacity; /* (親の行、ソース位置の識別子)を引く固定サイズの表。 */
    suz_arena_report_frame *active;
} suz_arena_tree;

void suz_arena_tree_init(suz_arena_tree *tree, size_t capacity, const char *source, size_t line, size_t column);
size_t suz_arena_tree_find(suz_arena_tree *tree, const suz_arena_report_site *site, size_t parent);

/* 初回・親の切替・再帰・表示省略の判定は、各ソース位置に複製しない。 */
void suz_arena_tree_enter_uncached(suz_arena_tree *tree, suz_arena_report_frame *frame);

/* 同じ親への再進入を小さく保ち、Cコンパイラが反復内へ展開できるようにする。
   強制展開はせず、多数のソース位置がある場合は関数本体の共有を許す。 */
static inline void suz_arena_tree_enter(
    suz_arena_tree *tree, suz_arena_report_frame *frame, size_t position, suz_arena_report_site *site) {
    *frame = (suz_arena_report_frame){ .previous = tree->active, .node = SIZE_MAX, .base = position, .peak = position, .site = site };
    tree->active = frame;
    size_t parent = frame->previous ? frame->previous->node : 0;
    if (!site->active && parent != SIZE_MAX && site->cached_parent == parent) {
        frame->node = site->cached_node;
        if (frame->node != SIZE_MAX) {
            frame->outermost = true;
            site->active = frame;
        }
    } else {
        suz_arena_tree_enter_uncached(tree, frame);
    }
}

static inline void suz_arena_tree_leave(suz_arena_tree *tree, suz_arena_report_frame *frame) {
    /* 再帰した内側の退出では、外側の基準位置とアクティブ状態を解除しない。 */
    if (frame->outermost) {
        size_t peak = frame->peak - frame->base;
        if (peak > tree->nodes[frame->node].peak) tree->nodes[frame->node].peak = peak;
        frame->site->active = NULL;
    }
    if (frame->previous && frame->peak > frame->previous->peak) frame->previous->peak = frame->peak;
    tree->active = frame->previous;
}
void suz_arena_tree_output(suz_arena_tree *tree, size_t peak, bool initial_block, suz_arena_report_format format);
void suz_arena_tree_destroy(suz_arena_tree *tree);
#endif
