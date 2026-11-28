#!/usr/bin/env python3
"""文書だけならCIの検査を省略し、コード変更や比較不能なら実行することを確かめる。

一時ディレクトリにテスト専用のGit履歴を作る。実際の作業ブランチは変更しない。
実行方法: python3 scripts/test-ci-changes.py
"""

import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import sys
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True

# ファイル名にハイフンがあるため、通常のimportではなくパスを指定して読み込む。
spec = importlib.util.spec_from_file_location('ci_changes', Path(__file__).with_name('ci-changes.py'))
ci = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ci)


class ChangeTests(unittest.TestCase):
    def test_documentation_and_executable_inputs(self):
        # 説明文だけなら省略できる。ただし、テストが読む仕様例やビルドに使う
        # 文書は、拡張子が.mdでも検査が必要。コードとの混在時も省略しない。
        self.assertFalse(ci.requires_checks(['HUMANS.md', 'docs/releasing.md', 'compiler/README.md']))
        self.assertFalse(ci.requires_checks([]))
        for path in ['README.md', 'docs/spec.md', 'docs/spec-cases.md', 'docs/spec-cases/flow-control.md',
                     'docs/spec-cases/new.md', 'runtime/third_party/ryu/README.md',
                     '.github/workflows/ci.yml', 'compiler/src/main.rs', 'Cargo.lock',
                     'scripts/ci-changes.py', 'LICENSE-MIT']:
            with self.subTest(path=path):
                self.assertTrue(ci.requires_checks([path]))
                self.assertTrue(ci.requires_checks(['compiler/README.md', path]))

    def test_manual_and_new_branch_runs(self):
        # 手動実行と新規ブランチには比較元がないので、必ず検査する。
        # GitHubは新規ブランチのbeforeを全桁0で通知する。
        # Noneは「変更なし」ではなく「ファイルで絞らず検査する」を意味する。
        self.assertIsNone(ci.changed_paths('workflow_dispatch', {}))
        self.assertIsNone(ci.changed_paths('push', {'before': '0' * 40}))

    def test_git_ranges_and_renamed_inputs(self):
        # CソースをREADME.mdに改名した2コミットを作る。
        # 新しい名前だけ見ると文書変更に見えるが、Cソースの削除も含むため検査が必要。
        with tempfile.TemporaryDirectory() as directory:
            def git(*args):
                return subprocess.check_output(['git', '-C', directory, *args], text=True).strip()
            git('init', '-q')
            git('config', 'user.name', 'CI test')
            git('config', 'user.email', 'ci@example.invalid')
            source = Path(directory, 'source.c')
            source.write_text('int main(void) { return 0; }\n')
            git('add', '.')
            git('commit', '-qm', 'Initial source')
            base = git('rev-parse', 'HEAD')
            source.rename(Path(directory, 'README.md'))
            git('add', '-A')
            git('commit', '-qm', 'Rename source')
            head = git('rev-parse', 'HEAD')
            # 検査本体が呼ぶgitも一時リポジトリを見るよう、この区間だけ環境変数を設定する。
            with patch.dict('os.environ', {'GIT_DIR': str(Path(directory, '.git')),
                                           'GIT_WORK_TREE': directory}):
                # pushとPRではGitHubの通知形式と差分の取り方が異なる。両方を確認する。
                for name, event in [
                    ('push', {'before': base, 'after': head}),
                    ('pull_request', {'pull_request': {'base': {'sha': base}, 'head': {'sha': head}}}),
                ]:
                    with self.subTest(event=name):
                        paths = ci.changed_paths(name, event)
                        self.assertEqual(set(paths), {'source.c', 'README.md'})
                        self.assertTrue(ci.requires_checks(paths))
                # 存在しないコミットIDで、履歴を戻した後の「比較元がない」状態を再現する。
                # 現在のコミットがあれば、例外で停止せず検査の実行を選ぶ。
                self.assertIsNone(ci.changed_paths('push', {'before': 'f' * 40, 'after': head}))
                # GitHub Actionsの入力・出力ファイルも再現し、後続ジョブへ
                # 「検査が必要」を伝えるrequired=trueが実際に書かれることを確認する。
                event_file = Path(directory, 'event.json')
                output_file = Path(directory, 'output.txt')
                event_file.write_text(json.dumps({'before': 'f' * 40, 'after': head}))
                with patch.dict('os.environ', {'GITHUB_EVENT_NAME': 'push',
                                               'GITHUB_EVENT_PATH': str(event_file),
                                               'GITHUB_OUTPUT': str(output_file)}):
                    ci.main()
                self.assertEqual(output_file.read_text(), 'required=true\n')
                # 現在のコミットまでない場合は検査対象が不明なので、失敗しなければならない。
                for event in [
                    {'before': base, 'after': 'f' * 40},
                    {'before': 'e' * 40, 'after': 'f' * 40},
                ]:
                    with self.subTest(event=event), self.assertRaises(subprocess.CalledProcessError):
                        ci.changed_paths('push', event)
                # 今回の救済はpushだけ。PRの比較元不在を黙って通すことはしない。
                with self.assertRaises(subprocess.CalledProcessError):
                    ci.changed_paths('pull_request', {
                        'pull_request': {'base': {'sha': 'f' * 40}, 'head': {'sha': head}},
                    })


if __name__ == '__main__':
    unittest.main()
