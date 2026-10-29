import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from niruflow.core import config
from niruflow.modules.forgotten import find_old_files
from niruflow.modules.doctor import check

class ReleaseTests(unittest.TestCase):
    def test_config_save(self):
        with tempfile.TemporaryDirectory() as d:
            with patch.object(config,"BASE",Path(d)),patch.object(config,"CONFIG",Path(d)/"config.json"):
                config.save(config.defaults())
                self.assertEqual(config.load()["theme"],"NIRUX desktop")
                self.assertEqual((Path(d)/"config.json").stat().st_mode & 0o777,0o600)
    def test_forgotten_does_not_follow_symlinks(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            (root/"example.txt").write_text("old")
            os.utime(root/"example.txt",(1,1))
            (root/"sym").symlink_to(root/"example.txt")
            result=find_old_files(root,months=3)
            self.assertEqual([Path(x["path"]).name for x in result],["example.txt"])
    def test_doctor(self):
        self.assertIn("python",check())
