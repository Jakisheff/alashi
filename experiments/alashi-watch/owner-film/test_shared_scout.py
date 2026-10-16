"""Offline regressions for the two observed workflow failure modes."""
import contextlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import shared_scout


class SharedScoutRegression(unittest.TestCase):
    def test_actual_fenced_snapshots_parse(self):
        receipts = Path(__file__).parent/'evidence/shared-scout-live/requests.jsonl'
        scouts = [row for row in map(json.loads, receipts.read_text().splitlines())
                  if row['role'] == 'scout' and row['response'].get('choices')]
        self.assertEqual(len(scouts), 2)
        for row in scouts:
            self.assertEqual(shared_scout.parse_answer(
                row['response']['choices'][0]['message']['content']), row['expected'])
        self.assertIsNone(shared_scout.parse_answer('Text before {"goods":4}'))
        self.assertIsNone(shared_scout.parse_answer('{"goods":'))

    def test_dependency_failure_never_sends_child_requests(self):
        real_run = subprocess.run
        real_read = Path.read_text
        sent = []

        def fake_run(command, **kwargs):
            if command[0] != 'curl':
                return real_run(command, **kwargs)
            sent.append(command)
            return subprocess.CompletedProcess(command, 22,
                '{"error":{"code":"1302","message":"test rejection"}}', '')

        def safe_read(path, *args, **kwargs):
            if path == Path.home()/'.config/alashi/llm.json':
                return '{"key":"offline-test-placeholder"}'
            return real_read(path, *args, **kwargs)

        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)/'result'
            with patch('sys.argv', ['shared_scout.py', '--live', '--out', str(out)]), \
                    patch.object(subprocess, 'run', side_effect=fake_run), \
                    patch.object(Path, 'read_text', safe_read), \
                    contextlib.redirect_stdout(io.StringIO()):
                shared_scout.main()
            self.assertEqual(len(sent), 6)
            rows = [json.loads(line) for line in (out/'requests.jsonl').read_text().splitlines()]
            self.assertEqual([r['role'] for r in rows],
                             ['market', 'scout', 'scout', 'market', 'market', 'scout'])
            self.assertTrue(all(not r['workflow_complete']
                                for r in json.loads((out/'runs.json').read_text())))
            self.assertIsNone(json.loads((out/'summary.json').read_text())['tokens_saved'])


if __name__ == '__main__':
    unittest.main()
