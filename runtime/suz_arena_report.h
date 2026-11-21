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
    size_t line, column;
    size_t parent, first_child, last_child, next_sibling, depth;
    size_t peak;
    bool recursive, omitted;
} suz_arena_report_node;

typedef struct suz_arena_report_frame {
    struct suz_arena_report_frame *previous;
    size_t node, base, peak;
    bool outermost;
} suz_arena_report_frame;

typedef struct {
    suz_arena_report_node *nodes;
    size_t *path; /* 出力用の祖先列。nodesと同じ一度の確保に含める。 */
    size_t count, capacity;
    suz_arena_report_frame *active;
} suz_arena_tree;

void suz_arena_tree_init(suz_arena_tree *tree, size_t capacity, const char *source, size_t line, size_t column);
void suz_arena_tree_enter(suz_arena_tree *tree, suz_arena_report_frame *frame, size_t position,
                          const char *source, size_t line, size_t column, const char *function);
void suz_arena_tree_leave(suz_arena_tree *tree, suz_arena_report_frame *frame);
void suz_arena_tree_output(suz_arena_tree *tree, size_t peak, bool initial_block, suz_arena_report_format format);
void suz_arena_tree_destroy(suz_arena_tree *tree);
#endif
