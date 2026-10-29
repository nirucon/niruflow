import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from niruflow.modules.storage import storage_delete, fingerprint

class StorageDeleteTests(unittest.TestCase):
    def row(self, path, st=None):
        st = st or path.stat()
        return {'path':str(path),'bytes':st.st_size,'mtime':st.st_mtime,
                'ctime_ns':st.st_ctime_ns,'inode':st.st_ino,'device':st.st_dev,
                'fingerprint':fingerprint(path)}
    def test_delete_regular_file(self):
        with tempfile.TemporaryDirectory(dir=Path.home()) as d:
            path=Path(d)/'test.txt';path.write_text('example')
            with patch('niruflow.core.store.record'):
                result=storage_delete([self.row(path)])
            self.assertEqual(result['deleted'],[str(path)])
            self.assertFalse(path.exists())
    def test_modified_file_is_protected(self):
        with tempfile.TemporaryDirectory(dir=Path.home()) as d:
            path=Path(d)/'test.txt';path.write_text('example')
            row=self.row(path)
            path.write_text('changed')
            result=storage_delete([row])
            self.assertFalse(result['deleted']);self.assertTrue(path.exists())
    def test_symlink_is_protected(self):
        with tempfile.TemporaryDirectory(dir=Path.home()) as d:
            real=Path(d)/'real';real.write_text('safe')
            link=Path(d)/'link';link.symlink_to(real)
            result=storage_delete([self.row(real) | {'path':str(link)}])
            self.assertFalse(result['deleted']);self.assertTrue(real.exists())
