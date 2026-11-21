#include "suz_arena_report.h"
#include "suz_arena.h"
#include "suz_runtime.h"
#include <assert.h>
#include <string.h>

/* 行数はCLIでは1..65535。公開Rust APIの値も同じ範囲で、計算量と容量に上限がある。 */
void suz_arena_tree_init(suz_arena_tree *tree, size_t capacity, const char *source, size_t line, size_t column) {
    assert(capacity > 0 && capacity <= UINT16_MAX);
    size_t bytes = capacity * (sizeof(suz_arena_report_node) + sizeof(size_t));
    tree->nodes = suz_env_alloc(bytes);
    if (!tree->nodes) suz_panic(source, line, column, "arena report allocation failed [rt.arena-report-storage]");
    tree->path = (size_t *)(tree->nodes + capacity);
    tree->capacity = capacity;
    tree->count = 1;
    tree->active = NULL;
    tree->nodes[0] = (suz_arena_report_node){
        .source = source, .function = "main", .line = line, .column = column,
        .parent = SIZE_MAX, .first_child = SIZE_MAX, .last_child = SIZE_MAX, .next_sibling = SIZE_MAX
    };
}

static bool same_site(const suz_arena_report_node *node, const char *source, size_t line, size_t column) {
    return node->line == line && node->column == column && strcmp(node->source, source) == 0;
}

void suz_arena_tree_enter(suz_arena_tree *tree, suz_arena_report_frame *frame, size_t position,
                          const char *source, size_t line, size_t column, const char *function) {
    *frame = (suz_arena_report_frame){ .previous = tree->active, .node = SIZE_MAX, .base = position, .peak = position };
    tree->active = frame;
    /* 省略した枝の内側を別の親へ付け替えない。計測値の伝播だけは通常どおり行う。 */
    if (frame->previous && frame->previous->node == SIZE_MAX) return;
    for (suz_arena_report_frame *ancestor = frame->previous; ancestor; ancestor = ancestor->previous) {
        if (same_site(&tree->nodes[ancestor->node], source, line, column)) {
            frame->node = ancestor->node;
            tree->nodes[frame->node].recursive = true;
            return;
        }
    }
    size_t parent = frame->previous ? frame->previous->node : 0;
    for (size_t child = tree->nodes[parent].first_child; child != SIZE_MAX; child = tree->nodes[child].next_sibling) {
        if (same_site(&tree->nodes[child], source, line, column)) {
            frame->node = child;
            frame->outermost = true;
            return;
        }
    }
    if (tree->count == tree->capacity) {
        tree->nodes[parent].omitted = true;
        return;
    }
    size_t id = tree->count++;
    tree->nodes[id] = (suz_arena_report_node){
        .source = source, .function = function, .line = line, .column = column,
        .parent = parent, .first_child = SIZE_MAX, .last_child = SIZE_MAX, .next_sibling = SIZE_MAX,
        .depth = tree->nodes[parent].depth + 1
    };
    if (tree->nodes[parent].last_child != SIZE_MAX) tree->nodes[tree->nodes[parent].last_child].next_sibling = id;
    else tree->nodes[parent].first_child = id;
    tree->nodes[parent].last_child = id;
    frame->node = id;
    frame->outermost = true;
}

void suz_arena_tree_leave(suz_arena_tree *tree, suz_arena_report_frame *frame) {
    assert(tree->active == frame);
    /* 各確保では現在のframeだけを更新し、退出で親へ最大位置を渡す。
       再帰を集約した行の基準は最も外側のframeだけが持つ。 */
    if (frame->outermost && frame->node != SIZE_MAX) {
        size_t peak = frame->peak - frame->base;
        if (peak > tree->nodes[frame->node].peak) tree->nodes[frame->node].peak = peak;
    }
    if (frame->previous && frame->peak > frame->previous->peak) frame->previous->peak = frame->peak;
    tree->active = frame->previous;
}

void suz_arena_tree_destroy(suz_arena_tree *tree) {
    suz_env_free(tree->nodes);
    *tree = (suz_arena_tree){0};
}

static void text(const char *value) { suz_log_text(value, strlen(value)); }
static size_t digits(size_t value) {
    size_t length = 1;
    while (value >= 10) { value /= 10; ++length; }
    return length;
}

/* テキスト表では制御文字・非ASCIIバイトをエスケープし、パスによる改行や桁ずれを防ぐ。
   CSVは元のUTF-8を保持し、引用符を二重にする。 */
static size_t escaped(const char *value, bool write) {
    size_t length = 0;
    for (const unsigned char *p = (const unsigned char *)value; *p; ++p) {
        if (*p < 32 || *p >= 127 || *p == '\\') {
            if (write) {
                const char hex[] = "0123456789abcdef";
                char escape[] = { '\\', 'x', hex[*p >> 4], hex[*p & 15] };
                suz_log_text(escape, sizeof(escape));
            }
            length += 4;
        } else {
            if (write) suz_log_text((const char *)p, 1);
            ++length;
        }
    }
    return length;
}

static void csv_string(const char *value) {
    text("\"");
    for (const char *p = value; *p; ++p) {
        if (*p == '"') text("\"");
        suz_log_text(p, 1);
    }
    text("\"");
}

static void spaces(size_t count) {
    while (count--) text(" ");
}

static size_t site(const suz_arena_report_node *node, size_t id, bool write) {
    if (id == 0) { if (write) text("main"); return 4; }
    size_t length = escaped(node->source, write) + 2 + digits(node->line) + digits(node->column);
    if (write) { text(":"); suz_log_u64(node->line); text(":"); suz_log_u64(node->column); }
    return length;
}

/* 整数の書式化は固定のローカル配列だけで行い、ロケールやstdioへ依存しない。 */
static char *decimal(char *out, size_t value) {
    char reversed[20];
    size_t count = 0;
    do { reversed[count++] = (char)('0' + value % 10); value /= 10; } while (value);
    while (count) *out++ = reversed[--count];
    *out = 0;
    return out;
}

static void amount(char buffer[64], size_t value, bool bytes) {
    if (bytes || value < 1024) {
        char *end = decimal(buffer, value);
        if (!bytes) strcpy(end, " B");
        return;
    }
    size_t unit = value < 1024 * 1024 ? 1024 : 1024 * 1024;
    size_t whole = value / unit;
    size_t tenth = ((value % unit) * 10 + unit / 2) / unit;
    if (tenth == 10) { ++whole; tenth = 0; }
    char *end = decimal(buffer, whole);
    *end++ = '.';
    *end++ = (char)('0' + tenth);
    strcpy(end, unit == 1024 ? " KiB" : " MiB");
}

static void csv_row(const suz_arena_report_node *node, size_t id, uint64_t reserved, size_t blocks, size_t expansions) {
    suz_log_u64(id); text(",");
    if (id) suz_log_u64(node->parent);
    text(","); suz_log_u64(node->depth); text(",");
    /* ソースと位置を別列にして、パス中のコロンと混同しない。 */
    csv_string(node->source); text(","); suz_log_u64(node->line); text(","); suz_log_u64(node->column); text(",");
    csv_string(node->function); text(","); suz_log_u64(node->peak);
    text(node->recursive ? ",true" : ",false");
    text(node->omitted ? ",true," : ",false,");
    if (id == 0) suz_log_u64(reserved);
    text(","); if (id == 0) suz_log_u64(blocks);
    text(","); if (id == 0) suz_log_u64(expansions);
}

void suz_arena_tree_output(suz_arena_tree *tree, size_t peak, bool initial_block, suz_arena_report_format format) {
    assert(tree->active == NULL);
    tree->nodes[0].peak = peak;
    size_t blocks = 0;
    for (suz_arena_chunk *chunk = suz_arena.first; chunk; chunk = chunk->next) ++blocks;
    size_t expansions = blocks - (initial_block ? 1 : 0);
    size_t site_width = 5, function_width = 8, amount_width = 4;
    for (size_t i = 0; i < tree->count; ++i) {
        const suz_arena_report_node *node = &tree->nodes[i];
        size_t width = node->depth * 4 + site(node, i, false);
        if (width > site_width) site_width = width;
        width = escaped(node->function, false);
        if (width > function_width) function_width = width;
        char buffer[64]; amount(buffer, node->peak, format == SUZ_ARENA_REPORT_TREE_BYTES);
        if (strlen(buffer) > amount_width) amount_width = strlen(buffer);
    }
    /* 全行を1つのロック内で出す。出力中に領域の確保も再帰呼出しも行わない。 */
    suz_log_report_begin();
    if (format == SUZ_ARENA_REPORT_CSV) text("id,parent,depth,source,line,column,function,peak,recursive,omitted,reserved,blocks,expansions");
    else {
        text("arena"); spaces(site_width - 5 + 2);
        text("function"); spaces(function_width - 8 + 2);
        spaces(amount_width - 4); text("peak");
    }
    size_t id = 0;
    for (;;) {
        text("\n");
        const suz_arena_report_node *node = &tree->nodes[id];
        if (format == SUZ_ARENA_REPORT_CSV) csv_row(node, id, suz_arena.total, blocks, expansions);
        else {
            size_t ancestor = id;
            for (size_t d = node->depth; d > 0; --d) {
                tree->path[d - 1] = ancestor;
                ancestor = tree->nodes[ancestor].parent;
            }
            for (size_t d = 0; d < node->depth; ++d) {
                bool last = tree->nodes[tree->path[d]].next_sibling == SIZE_MAX;
                text(d + 1 == node->depth ? (last ? "└── " : "├── ") : (last ? "    " : "│   "));
            }
            size_t width = node->depth * 4 + site(node, id, true);
            spaces(site_width - width + 2);
            width = escaped(node->function, true);
            spaces(function_width - width + 2);
            char buffer[64]; amount(buffer, node->peak, format == SUZ_ARENA_REPORT_TREE_BYTES);
            spaces(amount_width - strlen(buffer)); text(buffer);
            if (node->recursive) text(" [recursive]");
            if (node->omitted) text(" [omitted]");
        }
        if (node->first_child != SIZE_MAX) { id = node->first_child; continue; }
        while (id && tree->nodes[id].next_sibling == SIZE_MAX) id = tree->nodes[id].parent;
        if (!id) break;
        id = tree->nodes[id].next_sibling;
    }
    if (format != SUZ_ARENA_REPORT_CSV) {
        text("\nreserved="); suz_log_u64(suz_arena.total);
        text(" blocks="); suz_log_u64(blocks);
        text(" expansions="); suz_log_u64(expansions);
    }
    suz_log_end();
}
