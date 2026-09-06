import contextlib
import io
import json
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import patch
import anchor_dataset as anchor

class Verification(unittest.TestCase):
    def test_verify_detects_tampering_without_reanchoring(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'docs').mkdir()
            data = root/'sample.txt'
            data.write_text('original')
            with patch.object(anchor, 'ROOT', directory), patch.object(anchor, 'TARGETS', ['sample.txt']), contextlib.redirect_stdout(io.StringIO()):
                digest, files = anchor.build()
                record = root/'docs/anchored.json'
                record.write_text(json.dumps({'anchors':[{'root':digest,'files':files}]}))
                before = record.read_bytes()
                with patch('sys.argv',['anchor_dataset.py','--verify']):
                    self.assertEqual(anchor.main(),0)
                    data.write_text('tampered')
                    self.assertEqual(anchor.main(),1)
                self.assertEqual(record.read_bytes(),before)

    def test_missing_record_fails_without_creating_one(self):
        with TemporaryDirectory() as directory, patch.object(anchor, 'ROOT', directory), patch.object(anchor, 'TARGETS', ['missing.txt']), patch('sys.argv',['anchor_dataset.py']), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(anchor.main(),1)
            self.assertFalse((Path(directory)/'docs/anchored.json').exists())

if __name__ == '__main__':
    unittest.main()
