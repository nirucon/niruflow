import os
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

from niruflow.modules.cleanup import scan,quarantine,restore
from niruflow.modules.projects import analyze
from niruflow.core.safety import validate_root

class CoreTests(unittest.TestCase):
    def test_nonrecursive_protected(self):
        with tempfile.TemporaryDirectory(dir=Path.home()) as d:
            root=Path(d)
            old=root/"old.txt"
            old.write_text("data")
            folder=root/"Important"
            folder.mkdir()
            (folder/"saved.txt").write_text("keep")
            past=time.time()-100000
            os.utime(old,(past,past))
            results=scan({"path":str(root),"hours":24})
            self.assertEqual(results,[])  # freshly created file is protected by ctime gate
    def test_symlink_rejected(self):
        with tempfile.TemporaryDirectory(dir=Path.home()) as d:
            root=Path(d)
            (root/"target").write_text("data")
            (root/"link").symlink_to(root/"target")
            self.assertEqual(scan({"path":str(root),"hours":0.000001}),[])
    def test_project_versions(self):
        with tempfile.TemporaryDirectory(dir=Path.home()) as d:
            root=Path(d)/"nplay"
            root.mkdir()
            for v in ["0.8.0","0.9.0","0.9.1"]:
                (root/f"nplay-{v}.zip").write_text("x")
            out=analyze(d)
            self.assertEqual({x["version"]:x["status"] for x in out},{"0.8.0":"review","0.9.0":"keep","0.9.1":"keep"})
    def test_home_protected(self):
        with self.assertRaises(ValueError):
            validate_root(Path.home())

if __name__=="__main__":
    unittest.main()
