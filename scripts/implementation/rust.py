"""Rust構文に基づく実装規則の検査と、診断箇所の対応表の生成。"""

from collections import defaultdict
import json
from pathlib import PurePosixPath
import re
import subprocess

from .check import EXCEPTIONS, read_exceptions, repository_files

INDEX = 'compiler/tests/implementation-rule-index.json'
MANIFEST = 'scripts/rust-implementation/Cargo.toml'
RULES = {'impl.layers', 'impl.format', 'impl.module-doc', 'impl.rule-index', 'impl.diagnostic', 'impl.no-silent'}
LINTS = ('let_underscore_must_use', 'unused_result_ok', 'map_err_ignore')
RULE_ID = re.compile(r'\[([a-z][a-z0-9-]*(?:\.[a-z0-9-]+)+)\]')


def syntax(root, sources):
    """コンパイラ本体と分離した構文解析ツールに、Git上のRustソースを渡す。"""
    result = subprocess.run(
        ['cargo', 'run', '--quiet', '--locked', '--manifest-path', MANIFEST],
        cwd=root, input=json.dumps(sources), text=True, capture_output=True,
    )
    if result.returncode:
        raise ValueError(f'Rust syntax analysis failed:\n{result.stderr}')
    return json.loads(result.stdout)


def module_files(reports):
    """mod宣言をたどり、外部ファイルのモジュール名とテスト専用かを決める。"""
    result = {}
    pending = [('compiler/src/lib.rs', (), False), ('compiler/src/main.rs', (), False)]
    while pending:
        name, module, test_only = pending.pop()
        if name not in reports:
            raise ValueError(f'Module source is not in Git file list: {name}')
        key = (name, module, test_only)
        if key in result:
            continue
        result[key] = True
        file = PurePosixPath(name)
        directory = file.parent if file.stem in {'lib', 'main', 'mod'} else file.with_suffix('')
        for child in reports[name]['modules']:
            inline = tuple(child['module'])
            if child['path'] is not None:
                # #[path] は外部ファイルの親、インラインmod内ではそのディレクトリを基準とする。
                base = file.parent if not inline else directory.joinpath(*inline)
                pieces = []
                for part in (base / child['path']).parts:
                    if part == '..':
                        pieces.pop()
                    elif part != '.':
                        pieces.append(part)
                candidates = ['/'.join(pieces)]
            else:
                base = directory.joinpath(*inline, child['name'])
                candidates = [str(base.with_suffix('.rs')), str(base / 'mod.rs')]
            matches = [p for p in candidates if p in reports]
            if len(matches) != 1:
                raise ValueError(f'{name}: expected one source for module {child["name"]}: {candidates}')
            pending.append((matches[0], module + inline + (child['name'],), test_only or child['test_only']))
    production = {name: module for name, module, test in result if not test}
    tests = {name for name, _, test in result if test} - production.keys()
    # mod宣言にない新規ソースも、名前だけでテスト扱いして検査から外さない。
    for name in reports:
        if name.startswith('compiler/src/') and name not in production and name not in tests:
            parts = PurePosixPath(name).relative_to('compiler/src').with_suffix('').parts
            production[name] = parts[:-1] if parts[-1] == 'mod' else parts
    return production


def category(module):
    """型の定義と処理モジュールを区別する。入口の共通型は階層間の受け渡しに使える。"""
    if module[:1] == ('ast',):
        return ('data', 2, 'ast')
    if module[:2] == ('check', 'ir'):
        return ('data', 3, 'check::ir')
    if module[:1] == ('numeric',):
        return ('common', 0, 'numeric')
    layers = {'lexer': 1, 'parser': 2, 'check': 3, 'warnings': 4, 'codegen': 4, 'cli': 5}
    if module and module[0] in layers:
        return ('process', layers[module[0]], module[0])
    return ('common', 0, module[0]) if module else ('entry', 0, 'entry')


def forbidden_dependency(source, target):
    src, dst = category(source), category(target)
    if src[0] == 'entry' or dst[0] in {'entry', 'common'} or src == dst:
        return False
    if src[0] == 'common':
        return True
    if src[0] == 'data':
        return dst[0] == 'process' or dst[1] > src[1]
    return dst[1] > src[1] or (dst[0] == 'process' and dst[1] == src[1])


def dependencies(reports, production):
    """useの別名・グループ・再公開とcrate/self/superを解決して依存を列挙する。"""
    aliases = {}
    roots = {'ast', 'check', 'lexer', 'parser', 'warnings', 'codegen', 'cli', 'numeric'}
    for name, base in production.items():
        for item in reports[name]['imports']:
            scope = base + tuple(item['scope'])
            aliases[(scope, item['alias'])] = (tuple(item['path']), base + tuple(item['module']))

    def resolve(parts, module, scope, visited=frozenset()):
        if not parts or len(visited) > 40:
            return ()
        first, rest = parts[0], parts[1:]
        if first in {'crate', 'suz'}:
            target = rest
        elif first == 'self':
            target = module + rest
        elif first == 'super':
            parent = module
            while parts and parts[0] == 'super':
                parent, parts = parent[:-1], parts[1:]
            target = parent + parts
        else:
            for length in range(len(scope), -1, -1):
                key = (scope[:length], first)
                if key in aliases and key not in visited:
                    original, owner = aliases[key]
                    return resolve(original, owner, key[0], visited | {key}) + rest
            target = parts if first in roots else module + parts
        # useによる再公開を、定義元の型・処理へたどる。
        for length in range(len(target) - 1, -1, -1):
            key = (target[:length], target[length])
            if key in aliases and key not in visited:
                original, owner = aliases[key]
                return resolve(original, owner, key[0], visited | {key}) + target[length + 1:]
        return target

    edges = {}
    for name, base in production.items():
        for reference in reports[name]['references']:
            owner = base + tuple(reference['module'])
            target = resolve(tuple(reference['path']), owner, base + tuple(reference['scope']))
            if forbidden_dependency(owner, target):
                # 型名ごとでなく、依存先モジュールごとに既存の違反を管理する。
                item = '::'.join(owner) + ' -> ' + category(target)[2]
                edges.setdefault((name, item), reference['line'])
    return edges


def condition(site):
    """対応する規則IDに続く条件の説明をコメントから取り出す。意味の妥当性はレビューする。"""
    rule = site['rule']
    if not rule:
        return None
    for text in (site['comment'], *site.get('context_comments', []), site['function_doc']):
        for line in text.splitlines():
            marker = f'[{rule}]'
            if marker in line:
                explanation = line.split(marker, 1)[1].strip()
                if explanation:
                    return explanation
    return None


def structural_checks(sources, reports):
    """違反の識別子と、例外適用前の診断対応表を生成する。"""
    production = module_files(reports)
    violations, index = {}, []

    def add(rule, name, item, line, message):
        violations[(rule, name, item)] = f'{name}:{line}: {message} [{rule}]'

    for (name, item), line in dependencies(reports, production).items():
        add('impl.layers', name, item, line, f'Forbidden module dependency: {item}')
    for name, report in reports.items():
        for allowance in report['lint_allows']:
            add('impl.no-silent', name, allowance['item'], allowance['line'], 'Missing lint suppression reason')
        for skip in report['skips']:
            if not skip['comment'].strip():
                add('impl.format', name, f'rustfmt::skip:{skip["line"]}', skip['line'], 'Missing rustfmt skip reason')
        if name not in production:
            continue
        source = sources[name].removeprefix('\ufeff')
        if not source.startswith('//!'):
            add('impl.module-doc', name, None, 1, 'Missing leading //! module documentation')
        for site in report['diagnostics']:
            explanation = condition(site)
            row = {key: site[key] for key in ('item', 'kind', 'rule', 'rule_expression', 'line', 'column')}
            row.update(path=name, stage=category(production[name] + tuple(site['module']))[2], condition=explanation)
            index.append(row)
            if not site['rule'] or not RULE_ID.fullmatch(f'[{site["rule"]}]') or not explanation:
                add('impl.rule-index', name, site['item'], site['line'], 'Missing literal rule ID or condition comment')
            if any(RULE_ID.search(s) for s in site['strings']):
                add('impl.diagnostic', name, site['item'], site['line'], 'Rule ID embedded in diagnostic message')
    index.sort(key=lambda row: (row['path'], row['line'], row['column'], row['item']))
    return violations, index


def clippy_diagnostics(output, reports=None):
    """対象リントだけを例外と照合できる形に変換し、それ以外の警告・エラーは失敗にする。"""
    violations, errors, locations = {}, [], {}
    for line in output.splitlines():
        if not line.startswith('{'):
            continue
        event = json.loads(line)
        if event.get('reason') != 'compiler-message':
            continue
        message = event['message']
        if message['level'] not in {'warning', 'error'}:
            continue
        code = (message.get('code') or {}).get('code', '')
        if code not in {'clippy::' + lint for lint in LINTS}:
            errors.append(message.get('rendered') or message['message'])
            continue
        primary = next((s for s in message['spans'] if s['is_primary']), None)
        if not primary:
            errors.append(f'Clippy diagnostic without source location: {code}')
            continue
        name = primary['file_name']
        locations[(name, code, primary['line_start'], primary['column_start'])] = message['message']
    ordinals = {}
    for (name, code, line, column), message in sorted(locations.items()):
        scopes = [f for f in (reports or {}).get(name, {}).get('functions', [])
                  if f['start'] <= line <= f['end']]
        scope = '::'.join(max(scopes, key=lambda f: len(f['scope']))['scope']) if scopes else '<module>'
        key = (name, code, scope)
        ordinals[key] = ordinals.get(key, 0) + 1
        item = f'{scope}:{code}#{ordinals[key]}'
        violations[('impl.no-silent', name, item)] = f'{name}:{line}: {message} [impl.no-silent]'
    return violations, errors


def clippy_checks(root, reports):
    """Clippyは警告として全件を出し、検査側で例外以外を失敗にする。通常の-D warningsもCIに残す。"""
    command = ['cargo', 'clippy', '--locked', '--all-targets', '--message-format=json', '--']
    for lint in LINTS:
        command += ['-W', 'clippy::' + lint]
    result = subprocess.run(command, cwd=root, capture_output=True, text=True)
    violations, errors = clippy_diagnostics(result.stdout, reports)
    if result.returncode:
        errors.append(f'Clippy failed ({result.returncode}):\n{result.stderr}')
    return violations, errors


def check_index(root, index, *, write=False):
    """抽出した全診断箇所と保存済みの対応表を照合する。更新は明示した場合だけ行う。"""
    rendered = json.dumps(index, ensure_ascii=False, indent=2) + '\n'
    target = root / INDEX
    if write:
        target.write_text(rendered, encoding='utf-8')
        return []
    if not target.is_file() or target.read_text(encoding='utf-8') != rendered:
        return [f'{INDEX}: diagnostic index is stale; run with --write-index [impl.rule-index]']
    return []


def apply_exceptions(violations, entries):
    """1件の違反に1件の例外を対応させ、解消済み・過剰な例外を拒否する。"""
    remaining = dict(violations)
    errors = []
    for entry in entries:
        if entry['rule'] not in RULES:
            continue
        key = (entry['rule'], entry['path'], entry.get('item'))
        if key not in remaining:
            errors.append(f'{entry["path"]}: obsolete or unmatched exception {entry.get("item", "")} [{entry["rule"]}]')
        else:
            del remaining[key]
    return errors + list(remaining.values())


def inspect(root):
    """Gitに列挙されるソースだけを検査し、後続段階で解消する既存違反を照合する。"""
    files = repository_files(root)
    sources = {name: (root / name).read_text(encoding='utf-8')
               for name in sorted(files) if name.startswith('compiler/') and name.endswith('.rs')}
    reports = syntax(root, sources)
    violations, index = structural_checks(sources, reports)
    lint_violations, errors = clippy_checks(root, reports)
    violations.update(lint_violations)
    rules = set(re.findall(r'\*\*\[(impl\.[a-z-]+)\]\*\*', (root / 'docs/implementation.md').read_text()))
    entries = []
    read_exceptions((root / EXCEPTIONS).read_text(), rules, files=files, entries=entries)
    return violations, index, entries, errors
