#!/usr/bin/env python3
"""実装規則の行数検査と例外管理を検証する。"""

import json
from pathlib import Path
import subprocess
import shutil
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True

from implementation.check import (
    EXCEPTIONS, check_counts, check_repository, read_exceptions, source_counts, source_language,
)
from implementation.source import code_lines


def exception_text(**changes):
    """各テストが変更する項目を明示できる例外データを作る。"""
    entry = {
        'rule': 'impl.file-length',
        'path': 'compiler/src/example.rs',
        'reason': '既存ファイルの分割を別の変更としてレビューするため',
        'max_code_lines': 510,
    }
    entry.update(changes)
    return '[[exceptions]]\n' + '\n'.join(
        f'{key} = {json.dumps(value, ensure_ascii=False)}' for key, value in entry.items()
    ) + '\n'


class SourceTests(unittest.TestCase):
    # [impl.file-length] BOMを除き、Cの文字リテラルと数値を区別する。
    def test_rust_initial_bom(self):
        self.assertEqual(code_lines('\ufeff// bom\n', 'rust'), 0)
        self.assertEqual(code_lines('\ufeff', 'rust'), 0)
        self.assertEqual(code_lines('\ufefflet x = "\ufeff";\n', 'rust'), 1)
        self.assertEqual(code_lines('// comment\n\ufeff\n', 'rust'), 1)

    def test_c_prefixed_character_literals(self):
        for prefix in ('u8', 'u', 'U', 'L', ''):
            with self.subTest(prefix=prefix):
                self.assertEqual(code_lines(
                    f'char c = {prefix}\'a\'; char *s = "//";\n// comment\n', 'c'), 1)
        self.assertEqual(code_lines("int n = 1'000; char c = u8'/';\n", 'c'), 1)

    # [impl.file-length] 解析失敗は行継続前の物理行番号で報告する。
    def test_error_line_numbers(self):
        for source, language, expected in [
            ('// header\n/*', 'rust', 2),
            ('// header\nr#"text', 'rust', 2),
            ('// header\n"text', 'rust', 2),
            ('// header\n#error don\'t\n', 'c', 2),
            ('#if 0\ndon\'t\n#endif\n', 'c', 2),
            ('// header\n#include <a\'b.h>\n', 'c', 2),
            ('// header\\\ncontinued\n"text\\', 'c', 3),
        ]:
            for newline in ('\n', '\r\n'):
                with self.subTest(source=source, newline=newline):
                    with self.assertRaisesRegex(ValueError, f'line {expected}: Unterminated'):
                        code_lines(source.replace('\n', newline), language)

    # [impl.file-length] 空行・コメントのみの行と、コードを含む行を区別する。
    def test_comments_and_code_on_the_same_line(self):
        source = '''// comment

/* first
   middle
   last */
/* before */ int x; // after
int y; /* first
        last */ int z;
'''
        for language in ('rust', 'c'):
            with self.subTest(language=language):
                self.assertEqual(code_lines(source, language), 3)
                self.assertEqual(code_lines(source.replace('\n', '\r\n'), language), 3)

    def test_nested_comments_only_in_rust(self):
        source = '/* outer\n/* inner */\ncode\n*/'
        self.assertEqual(code_lines(source, 'rust'), 0)
        self.assertEqual(code_lines(source, 'c'), 2)

    def test_quoted_comment_markers_and_escaped_quotes(self):
        source = r'''let a = "// not a comment";
let b = "\"/* not a comment */";
let c = '/'; // comment
'''
        for language in ('rust', 'c'):
            self.assertEqual(code_lines(source, language), 3)

    def test_rust_raw_strings_and_byte_and_c_prefixes(self):
        for prefix in ('r', 'br', 'cr'):
            for hashes in ('', '#', '###'):
                source = f'let x = {prefix}{hashes}"first\n// data\n/* data */\n"{hashes};\n// comment'
                with self.subTest(prefix=prefix, hashes=hashes):
                    self.assertEqual(code_lines(source, 'rust'), 4)
        self.assertEqual(code_lines('r##"\n"# // still data\n"##', 'rust'), 3)

    def test_rust_lifetimes_labels_and_character_escapes(self):
        source = r'''fn x<'a>(x: &'a str) {
    'again: loop { break 'again; }
    let chars = ['\'', '\\', '\u{2f}', '\x2f', b'/'];
}
'''
        self.assertEqual(code_lines(source, 'rust'), 4)

    def test_multiline_literals_are_code(self):
        source = '"first\n// data\n\n/* data */\nlast"'
        self.assertEqual(code_lines(source, 'rust'), 4)
        self.assertEqual(code_lines('"text\\\n// data"', 'rust'), 2)
        self.assertEqual(code_lines('"text\\\n// data"', 'c'), 2)

    def test_c_splices_before_comment_recognition(self):
        cases = [
            ('// comment\\\ncontinued\nint x;', 1),
            ('/\\\n* comment\n*/\nint x;', 1),
            ('/\\\n/ comment\nint x;', 1),
            ('#define X \\\n1', 2),
        ]
        for source, expected in cases:
            for newline in ('\n', '\r\n'):
                with self.subTest(source=source, newline=newline):
                    self.assertEqual(code_lines(source.replace('\n', newline), 'c'), expected)

    def test_c23_digit_separators(self):
        source = "unsigned x = 1'000; // comment\nfloat y = 0x1'abp+2;\nint c = '/';"
        self.assertEqual(code_lines(source, 'c'), 3)

    def test_empty_files_and_unterminated_tokens(self):
        self.assertEqual(code_lines('', 'rust'), 0)
        self.assertEqual(code_lines('// final comment', 'rust'), 0)
        self.assertEqual(code_lines('code', 'rust'), 1)
        for source, language in [('/*', 'rust'), ('/*', 'c'), ('"text', 'rust'),
                                 ('r#"text', 'rust'), ('"text\\', 'c')]:
            with self.subTest(source=source):
                with self.assertRaises(ValueError):
                    code_lines(source, language)


class ExceptionTests(unittest.TestCase):
    # [impl.exceptions] 誤記を黙認せず、必要な例外だけを適用する。
    def test_valid_and_empty_lists(self):
        self.assertEqual(read_exceptions(''), {})
        self.assertEqual(read_exceptions('# All exceptions resolved.\n'), {})
        self.assertEqual(read_exceptions('exceptions = []'), {})
        self.assertEqual(read_exceptions(exception_text()), {'compiler/src/example.rs': 510})
        self.assertEqual(read_exceptions(exception_text(item='a_function')),
                         {'compiler/src/example.rs': 510})

    def test_other_rules_validate_without_applying_line_limits(self):
        # [impl.exceptions] 共通一覧には後続段階で検査する規則も保存できる。
        other = exception_text(rule='impl.format', item='a_function').replace(
            'max_code_lines = 510\n', '')
        rules = {'impl.format', 'impl.file-length'}
        self.assertEqual(read_exceptions(other, rules), {})
        self.assertEqual(read_exceptions(other + exception_text(), rules),
                         {'compiler/src/example.rs': 510})
        self.assertEqual(read_exceptions(other + other.replace('a_function', 'b_function'), rules), {})
        for invalid in [other + other, other.replace('impl.format', 'impl.unknown'),
                        other.replace('reason = ', 'unknown = '),
                        other.replace('"a_function"', '3'),
                        other.replace('compiler/src/example.rs', '../outside.rs'),
                        other + 'max_code_lines = 510\n']:
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                read_exceptions(invalid, rules)

    def test_invalid_fields_and_values(self):
        for change in [
            {'rule': '[impl.file-length]'}, {'rule': 1}, {'reason': ''}, {'reason': '  '}, {'reason': 1},
            {'max_code_lines': 500}, {'max_code_lines': True}, {'max_code_lines': '510'},
            {'path': '/compiler/src/example.rs'}, {'path': '../compiler/src/example.rs'},
            {'path': 'compiler/../runtime/a.c'}, {'path': 'compiler//a.rs'},
            {'path': './compiler/a.rs'}, {'path': 'compiler\\a.rs'}, {'path': 1},
            {'path': 'runtime/third_party/a.c'}, {'path': 'docs/example.md'},
            {'max_lines': 600}, {'item': ''}, {'item': 1},
        ]:
            with self.subTest(change=change):
                with self.assertRaises(ValueError):
                    read_exceptions(exception_text(**change))
        for text in ['exceptions = 3', 'exceptions = [1]',
                     exception_text() + exception_text(),
                     exception_text().replace('max_code_lines = 510', ''),
                     'unknown = 1\n' + exception_text()]:
            with self.subTest(text=text), self.assertRaises(ValueError):
                read_exceptions(text)

    def test_limit_boundaries(self):
        name = 'compiler/src/example.rs'
        self.assertEqual(check_counts({name: 500}, {}), [])
        self.assertIn('[impl.file-length]', check_counts({name: 501}, {})[0])
        self.assertEqual(check_counts({name: 510}, {name: 510}), [])
        self.assertEqual(check_counts({name: 501}, {name: 510}), [])
        self.assertIn('[impl.file-length]', check_counts({name: 511}, {name: 510})[0])

    def test_obsolete_and_missing_exceptions(self):
        name = 'compiler/src/example.rs'
        for counts in ({name: 500}, {name: 0}, {}):
            with self.subTest(counts=counts):
                errors = check_counts(counts, {name: 510})
                self.assertEqual(len(errors), 1)
                self.assertIn('[impl.exceptions]', errors[0])

    def test_source_scope(self):
        for name in ('compiler/build.rs', 'compiler/tests/test.rs', 'compiler/src/a.rs'):
            self.assertEqual(source_language(name), 'rust')
        for name in ('runtime/a.c', 'runtime/a.h', 'runtime/sub/a.c'):
            self.assertEqual(source_language(name), 'c')
        for name in ('runtime/third_party/a.c', 'docs/a.rs', 'compiler/a.md', 'a.c'):
            self.assertIsNone(source_language(name))


class RepositoryTests(unittest.TestCase):
    # [impl.file-length] 実際のGit索引と未追跡ファイルを使い、列挙から判定まで検証する。
    def test_git_sources_and_exception_lifecycle(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(['git', 'init', '-q', directory], check=True)

            def write(name, text):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text, encoding='utf-8')

            write('.gitignore', 'compiler/generated/\n')
            write('docs/implementation.md', '**[impl.file-length]**\n')
            write('compiler/src/example.rs', 'code\n' * 510)
            subprocess.run(['git', '-C', directory, 'add', '.'], check=True)
            write('compiler/src/new.rs', 'code\n' * 501)
            write('compiler/generated/ignored.rs', 'code\n' * 1000)
            write('runtime/third_party/library.c', 'code\n' * 1000)
            write('runtime/local.h', '/* header */\nint x;\n')
            write(EXCEPTIONS, exception_text())
            counts, exceptions, errors = check_repository(root)
            self.assertEqual(counts, {'compiler/src/example.rs': 510,
                                     'compiler/src/new.rs': 501, 'runtime/local.h': 1})
            self.assertEqual(len(errors), 1)
            self.assertIn('compiler/src/new.rs', errors[0])
            write('compiler/src/new.rs', 'code\n' * 500)
            self.assertEqual(check_repository(root)[2], [])
            write('compiler/src/example.rs', 'code\n' * 500)
            self.assertIn('obsolete exception', check_repository(root)[2][0])
            (root / 'compiler/src/example.rs').unlink()
            self.assertIn('exception source is missing', check_repository(root)[2][0])
            write(EXCEPTIONS, '# All exceptions resolved.\n')
            self.assertEqual(check_repository(root)[2], [])

    def test_command_exit_status_and_output(self):
        # [impl.exceptions] コマンドが検査の失敗を終了コードと標準エラーで報告する。
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(['git', 'init', '-q', directory], check=True)
            scripts = Path(__file__).resolve().parent
            (root / 'scripts').mkdir()
            shutil.copy(scripts / 'check-implementation.py', root / 'scripts')
            shutil.copytree(scripts / 'implementation', root / 'scripts/implementation')
            (root / 'docs').mkdir()
            shutil.copy(scripts.parent / 'docs/implementation.md', root / 'docs')
            source = root / 'compiler/src/example.rs'
            source.parent.mkdir(parents=True)
            config = root / EXCEPTIONS
            config.parent.mkdir(parents=True)
            source.write_text('code\n' * 510, encoding='utf-8')
            config.write_text(exception_text(), encoding='utf-8')

            def run():
                return subprocess.run(
                    [sys.executable, str(root / 'scripts/check-implementation.py')],
                    cwd=root, capture_output=True, text=True,
                )

            result = run()
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('1 exceptions verified', result.stdout)
            source.write_text('code\n' * 511, encoding='utf-8')
            result = run()
            self.assertEqual(result.returncode, 1)
            self.assertIn('[impl.file-length]', result.stderr)
            self.assertEqual(result.stdout, '')
            config.write_text('invalid = true', encoding='utf-8')
            result = run()
            self.assertEqual(result.returncode, 1)
            self.assertIn('Implementation check failed:', result.stderr)
            self.assertNotIn('Traceback', result.stderr)

    def test_source_errors_fail_with_path(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(['git', 'init', '-q', directory], check=True)
            (root / 'compiler').mkdir()
            (root / 'compiler/broken.rs').write_text('// header\n/*', encoding='utf-8')
            with self.assertRaisesRegex(ValueError, r'compiler/broken.rs: line 2:'):
                source_counts(root)


if __name__ == '__main__':
    unittest.main()
