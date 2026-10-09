import tempfile
import unittest
from pathlib import Path
from niruflow.modules.forgotten import find_old_files
from niruflow.modules.storage import files, largest, duplicates

class CancellationTests(unittest.TestCase):
    def test_storage_cancellation(self):
        with tempfile.TemporaryDirectory() as temp:
            Path(temp, 'test.txt').write_text('data')
            self.assertEqual(files(temp, cancelled=lambda: True), [])
            self.assertEqual(largest([temp], cancelled=lambda: True), [])
            self.assertEqual(duplicates([temp], cancelled=lambda: True), [])
    def test_old_files_cancellation(self):
        with tempfile.TemporaryDirectory() as temp:
            Path(temp, 'test.txt').write_text('data')
            self.assertEqual(find_old_files(temp, cancelled=lambda: True), [])
    def test_progress(self):
        with tempfile.TemporaryDirectory() as temp:
            Path(temp, 'test.txt').write_text('data')
            calls=[]
            files(temp, progress=calls.append)
            self.assertTrue(calls)
