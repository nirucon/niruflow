import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from niruflow.modules.storage import storage_delete

class StorageDeleteTests(unittest.TestCase):
    def test_delete_regular_file(self):
        with tempfile.TemporaryDirectory(dir=Path.home()) as d:
            path=Path(d)/'test.txt';path.write_text('example')
            st=path.stat()
            with patch('niruflow.modules.storage.storage_delete' if False else 'niruflow.core.store.record'):
                result=storage_delete([{'path':str(path),'bytes':st.st_size,'mtime':st.st_mtime}])
            self.assertEqual(result['deleted'],[str(path)])
            self.assertFalse(path.exists())
    def test_modified_file_is_protected(self):
        with tempfile.TemporaryDirectory(dir=Path.home()) as d:
            path=Path(d)/'test.txt';path.write_text('example')
            st=path.stat();path.write_text('changed')
            result=storage_delete([{'path':str(path),'bytes':st.st_size,'mtime':st.st_mtime}])
            self.assertFalse(result['deleted']);self.assertTrue(path.exists())
    def test_symlink_is_protected(self):
        with tempfile.TemporaryDirectory(dir=Path.home()) as d:
            real=Path(d)/'real';real.write_text('safe')
            link=Path(d)/'link';link.symlink_to(real)
            st=real.stat()
            result=storage_delete([{'path':str(link),'bytes':st.st_size,'mtime':st.st_mtime}])
            self.assertFalse(result['deleted']);self.assertTrue(real.exists())
