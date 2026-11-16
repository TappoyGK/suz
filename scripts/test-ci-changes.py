#!/usr/bin/env python3
"""Regression checks for CI filtering and Git change detection."""

import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import sys
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location('ci_changes', Path(__file__).with_name('ci-changes.py'))
ci = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ci)


class ChangeTests(unittest.TestCase):
    def test_documentation_and_executable_inputs(self):
        self.assertFalse(ci.requires_checks(['HUMANS.md', 'docs/releasing.md', 'compiler/README.md']))
        self.assertFalse(ci.requires_checks([]))
        for path in ['docs/spec.md', 'docs/spec-cases.md', 'docs/spec-cases/flow.md',
                     'docs/spec-cases/new.md', 'runtime/third_party/ryu/README.md',
                     '.github/workflows/ci.yml', 'compiler/src/main.rs', 'Cargo.lock',
                     'scripts/ci-changes.py', 'LICENSE-MIT']:
            with self.subTest(path=path):
                self.assertTrue(ci.requires_checks(['README.md', path]))

    def test_manual_and_new_branch_runs(self):
        self.assertIsNone(ci.changed_paths('workflow_dispatch', {}))
        self.assertIsNone(ci.changed_paths('push', {'before': '0' * 40}))

    def test_git_ranges_and_renamed_inputs(self):
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
            # Use a Git work tree without changing the test process directory.
            with patch.dict('os.environ', {'GIT_DIR': str(Path(directory, '.git')),
                                           'GIT_WORK_TREE': directory}):
                for name, event in [
                    ('push', {'before': base, 'after': head}),
                    ('pull_request', {'pull_request': {'base': {'sha': base}, 'head': {'sha': head}}}),
                ]:
                    with self.subTest(event=name):
                        paths = ci.changed_paths(name, event)
                        self.assertEqual(set(paths), {'source.c', 'README.md'})
                        self.assertTrue(ci.requires_checks(paths))
                self.assertIsNone(ci.changed_paths('push', {'before': 'f' * 40, 'after': head}))
                event_file = Path(directory, 'event.json')
                output_file = Path(directory, 'output.txt')
                event_file.write_text(json.dumps({'before': 'f' * 40, 'after': head}))
                with patch.dict('os.environ', {'GITHUB_EVENT_NAME': 'push',
                                               'GITHUB_EVENT_PATH': str(event_file),
                                               'GITHUB_OUTPUT': str(output_file)}):
                    ci.main()
                self.assertEqual(output_file.read_text(), 'required=true\n')
                for event in [
                    {'before': base, 'after': 'f' * 40},
                    {'before': 'e' * 40, 'after': 'f' * 40},
                ]:
                    with self.subTest(event=event), self.assertRaises(subprocess.CalledProcessError):
                        ci.changed_paths('push', event)
                with self.assertRaises(subprocess.CalledProcessError):
                    ci.changed_paths('pull_request', {
                        'pull_request': {'base': {'sha': 'f' * 40}, 'head': {'sha': head}},
                    })


if __name__ == '__main__':
    unittest.main()
