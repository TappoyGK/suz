#!/usr/bin/env python3
"""[impl.*] Rust構文からの規則判定、診断対応表、例外の有効範囲を検証する。"""

from pathlib import Path
import json
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True

from implementation.rust import INDEX, LINTS, apply_exceptions, check_index, clippy_diagnostics, condition, structural_checks, syntax

ROOT = Path(__file__).resolve().parent.parent


class RustRuleTests(unittest.TestCase):
    def check(self, files):
        files = {'compiler/src/lib.rs': '//! 入口。\n', 'compiler/src/main.rs': '//! CLI入口。\n', **files}
        return structural_checks(files, syntax(ROOT, files))

    def test_layers_resolve_aliases_groups_and_reexports(self):
        files = {
            'compiler/src/lib.rs': '//! 入口。\nmod ast; mod parser; mod check; mod warnings; mod codegen; mod numeric;',
            'compiler/src/ast.rs': '//! AST。\nuse crate as root; struct T(root::warnings::Warning);',
            'compiler/src/parser.rs': '//! Parser。\nuse crate::{check as later, ast::T}; fn f() { later::run(); }',
            'compiler/src/check.rs': '//! Check。\npub mod ir; pub use ir::Checked; fn run() {}',
            'compiler/src/check/ir.rs': '//! IR。\nstruct Checked;',
            'compiler/src/warnings.rs': '//! Warnings。\nuse crate::check::Checked; struct Warning;',
            'compiler/src/codegen.rs': '//! Codegen。\nuse crate::{warnings::Warning, check::Checked};',
            'compiler/src/numeric.rs': '//! Numeric。\nuse crate::ast::*;',
        }
        violations, _ = self.check(files)
        edges = {(path, item) for rule, path, item in violations if rule == 'impl.layers'}
        self.assertEqual(edges, {
            ('compiler/src/ast.rs', 'ast -> warnings'),
            ('compiler/src/parser.rs', 'parser -> check'),
            ('compiler/src/codegen.rs', 'codegen -> warnings'),
            ('compiler/src/numeric.rs', 'numeric -> ast'),
        })

    def test_ir_cannot_reference_check_processing(self):
        violations, _ = self.check({
            'compiler/src/lib.rs': '//! Root。\nmod check;',
            'compiler/src/check.rs': '//! Check。\nmod ir; mod exact;',
            'compiler/src/check/ir.rs': '//! IR。\nstruct V(super::exact::Exact);',
            'compiler/src/check/exact.rs': '//! Exact。\nstruct Exact;',
        })
        self.assertIn(('impl.layers', 'compiler/src/check/ir.rs', 'check::ir -> check'), violations)

    def test_local_variables_named_like_modules_do_not_create_dependencies(self):
        violations, _ = self.check({
            'compiler/src/lib.rs': '//! Root。\nmod parser;',
            'compiler/src/parser.rs': '//! Parser。\nfn f() { let check = 1; let codegen = check; }',
        })
        self.assertEqual(violations, {})

    def test_module_docs_exclude_test_modules_not_similarly_named_production_files(self):
        violations, _ = self.check({
            'compiler/src/lib.rs': '//! Root。\n#[cfg(test)] #[path = "testing.rs"] mod tests; mod tests_in_name;',
            'compiler/src/testing.rs': 'fn test() {}',
            'compiler/src/tests_in_name.rs': 'fn production() {}',
            'compiler/src/unreferenced.rs': 'fn new() {}',
        })
        missing = {path for rule, path, _ in violations if rule == 'impl.module-doc'}
        self.assertEqual(missing, {'compiler/src/tests_in_name.rs', 'compiler/src/unreferenced.rs'})

    def test_missing_module_file_fails(self):
        with self.assertRaisesRegex(ValueError, 'expected one source'):
            self.check({'compiler/src/lib.rs': '//! Root。\nmod absent;'})

    def test_bom_is_accepted_but_late_module_documentation_is_not(self):
        violations, _ = self.check({'compiler/src/lib.rs': '\ufeff//! Root。\n'})
        self.assertEqual(violations, {})
        violations, _ = self.check({'compiler/src/lib.rs': '// ordinary\n//! Late。\n'})
        self.assertIn(('impl.module-doc', 'compiler/src/lib.rs', None), violations)

    def test_diagnostic_index_contains_each_site_and_condition(self):
        violations, index = self.check({'compiler/src/lib.rs': '''//! Root。
fn f() {
    // [a.b] 条件を判定する。
    return Err(crate::diagnostic!("a.b", "message"));
}
/// [c.d] 別の条件を判定する。
fn g() { Diagnostic::new(k, 1, 2, "c.d", "message"); }
'''})
        self.assertEqual(violations, {})
        self.assertEqual([r['rule'] for r in index], ['a.b', 'c.d'])
        self.assertTrue(all(row['condition'] and row['path'] and row['stage'] for row in index))
        self.assertEqual(len({row['item'] for row in index}), 2)

    def test_dynamic_rules_and_missing_conditions_are_recorded_as_violations(self):
        violations, index = self.check({'compiler/src/lib.rs': '''//! Root。
fn f() { diagnostic!(rule, "message"); diagnostic!("a.b", "message"); }
'''})
        self.assertEqual(len(index), 2)
        self.assertEqual(len(violations), 2)
        self.assertIsNone(index[0]['rule'])
        self.assertEqual(index[0]['rule_expression'], 'rule')

    def test_embedded_rule_ids_in_diagnostic_messages_only(self):
        violations, _ = self.check({'compiler/src/lib.rs': r'''//! Root。
fn f() {
    diagnostic!("a.b", "\u{5b}a.b]");
    Diagnostic::new(k, 1, 2, "a.b", format!("message [a.b]"));
    let generated_c = "panic [a.b]";
    let source = r##"diagnostic!("a.b", "[a.b]")"##;
}
'''})
        self.assertEqual(sum(rule == 'impl.diagnostic' for rule, _, _ in violations), 2)

    def test_skip_reason_and_lookalikes(self):
        violations, _ = self.check({'compiler/src/lib.rs': '''//! Root。
// 表の列を揃えるため。
#[rustfmt::skip] fn f() {}
#[rustfmt::skip] fn g() {}
fn h() { let text = "#[rustfmt::skip]"; }
'''})
        self.assertEqual(sum(rule == 'impl.format' for rule, _, _ in violations), 1)

    def test_rule_ids_embedded_through_format_arguments_are_rejected(self):
        violations, index = self.check({'compiler/src/lib.rs': r'''//! Root。
fn f() {
    // [a.b] 条件を確認する。
    diagnostic!("a.b", "message {}", "[a.b]");
    // [a.c] 別の条件を確認する。
    diagnostic!("a.c", "message {detail}", detail = concat!("[a", ".c]"));
}
'''})
        self.assertEqual(len(index), 2)
        self.assertEqual(len(violations), 2)
        self.assertTrue(all(rule == 'impl.diagnostic' for rule, _, _ in violations))

    def test_exceptions_match_one_violation_and_fail_after_resolution(self):
        key = ('impl.rule-index', 'compiler/src/lib.rs', 'f:diagnostic!:a.b#1')
        entry = dict(rule=key[0], path=key[1], item=key[2], reason='段階5で対応する。')
        self.assertEqual(apply_exceptions({key: 'missing'}, [entry]), [])
        self.assertEqual(apply_exceptions({key: 'missing'}, []), ['missing'])
        self.assertIn('obsolete or unmatched', apply_exceptions({}, [entry])[0])
        other = ('impl.rule-index', key[1], 'g:diagnostic!:a.b#1')
        self.assertEqual(apply_exceptions({key: 'missing', other: 'new'}, [entry]), ['new'])

    def test_wrong_rule_comment_does_not_fill_condition(self):
        self.assertIsNone(condition(dict(rule='a.b', comment='[a.c] 別の規則。', function_doc='')))

    def test_saved_index_rejects_missing_changed_and_extra_rows(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / INDEX).parent.mkdir(parents=True)
            rows = [{'rule': 'a.b', 'condition': '条件。', 'path': 'compiler/src/lib.rs', 'line': 3}]
            self.assertTrue(check_index(root, rows))
            self.assertEqual(check_index(root, rows, write=True), [])
            self.assertEqual(check_index(root, rows), [])
            self.assertTrue(check_index(root, []))
            self.assertTrue(check_index(root, rows + rows))
            self.assertTrue(check_index(root, [dict(rows[0], condition='別の条件。')]))

    def test_self_constructor_is_indexed_and_message_is_checked(self):
        violations, index = self.check({'compiler/src/lib.rs': """//! Root.
impl Diagnostic {
    fn mk() -> Self {
        // [lex.x] cond
        Self::new(a, b, c, "lex.x", "bad [lex.x]")
    }
}
impl Other { fn mk() { Self::new(a, b, c, "lex.x", "bad [lex.x]"); } }
"""})
        self.assertEqual(len(index), 1)
        self.assertEqual(index[0]['condition'], 'cond')
        self.assertEqual([r for r, _, _ in violations], ['impl.diagnostic'])

    def test_reasonless_lint_suppression_is_rejected(self):
        violations, _ = self.check({'compiler/src/lib.rs': """//! Root.
#[allow(dead_code, clippy::let_underscore_must_use)]
fn missing() { let _ = "1".parse::<u8>(); }
#[allow(clippy::let_underscore_must_use, reason = "Caller handles failure.")]
fn valid() { let _ = "1".parse::<u8>(); }
"""})
        self.assertEqual(len(violations), 1)
        self.assertEqual(next(iter(violations))[0], 'impl.no-silent')

    def test_clippy_items_survive_unrelated_line_insertions(self):
        name = 'compiler/src/lib.rs'
        source = "//! Root.\nimpl T {\nfn f() {\na();\nb();\n}\n}\nfn g() { c(); }\n"
        def check(prefix):
            reports = syntax(ROOT, {name: prefix + source})
            events = []
            for line in (4, 5, 8):
                events.append(json.dumps({'reason': 'compiler-message', 'message': {
                    'code': {'code': 'clippy::map_err_ignore'}, 'level': 'warning', 'message': 'discarded',
                    'spans': [{'is_primary': True, 'file_name': name,
                               'line_start': line + prefix.count('\n'), 'column_start': 1}]}}))
            result, errors = clippy_diagnostics('\n'.join(events + events), reports)
            self.assertEqual(errors, [])
            return set(result)
        expected = {('impl.no-silent', name, item) for item in (
            'T::f:clippy::map_err_ignore#1', 'T::f:clippy::map_err_ignore#2', 'g:clippy::map_err_ignore#1')}
        self.assertEqual(check(''), expected)
        self.assertEqual(check('// unrelated\n\n'), expected)

    def test_group_and_macro_suppressions_are_rejected(self):
        violations, _ = self.check({'compiler/src/lib.rs': """//! Root.
#[allow(clippy::restriction)]
pub fn grouped() { let _ = "1".parse::<u8>(); }
fn f() { vec![#[allow(clippy::map_err_ignore)] { 1 }]; }
"""})
        self.assertEqual(len(violations), 2)
        self.assertTrue(all(rule == 'impl.no-silent' for rule, _, _ in violations))

    def test_extra_violation_reports_scope_and_count_not_wrong_line(self):
        name = 'compiler/src/check/exact.rs'
        violations = {
            ('impl.no-silent', name, f'build:clippy::map_err_ignore#{n}'):
                f'{name}:{line}: discarded [impl.no-silent]'
            for n, line in enumerate((97, 101, 123), 1)
        }
        entries = [dict(rule='impl.no-silent', path=name, item=f'build:clippy::map_err_ignore#{n}')
                   for n in (1, 2)]
        errors = apply_exceptions(violations, entries)
        self.assertEqual(len(errors), 1)
        self.assertIn('build: clippy::map_err_ignore: 3 violations, 2 exceptions, 1 over limit', errors[0])
        self.assertNotIn(':123:', errors[0])
        self.assertIn('cannot identify', errors[0])
        # 例外がなければ、各違反の場所をそのまま報告する。
        self.assertEqual(apply_exceptions(violations, []), list(violations.values()))

    def test_clippy_target_lints_and_other_warnings(self):
        def message(code, level='warning', spans=True):
            return json.dumps({'reason': 'compiler-message', 'message': {
                'code': {'code': code}, 'level': level, 'message': 'discarded',
                'spans': [{'is_primary': True, 'file_name': 'compiler/src/lib.rs',
                           'line_start': 7, 'column_start': 4}] if spans else [],
            }})
        output = '\n'.join(message('clippy::' + lint) for lint in LINTS)
        violations, errors = clippy_diagnostics(output + '\n' + output)
        self.assertEqual(len(violations), 3)
        self.assertEqual(errors, [])
        entries = [dict(rule=r, path=p, item=i) for r, p, i in violations]
        self.assertEqual(apply_exceptions(violations, entries), [])
        self.assertEqual(len(apply_exceptions(violations, entries[:-1])), 1)
        self.assertTrue(clippy_diagnostics(message('unused_imports'))[1])
        self.assertTrue(clippy_diagnostics(message('E0001', 'error'))[1])
        self.assertTrue(clippy_diagnostics(message('clippy::' + LINTS[0], spans=False))[1])


if __name__ == '__main__':
    unittest.main()
