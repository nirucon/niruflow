import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from niruflow.core import backup, scheduler

class V050Tests(unittest.TestCase):
    def test_backup_roundtrip_and_no_overwrite(self):
        with tempfile.TemporaryDirectory() as temp:
            target = Path(temp)/'backup.zip'
            cfg={'rules': [], 'profiles': {'default': {'locations': []}}, 'active_profile':'default'}
            with patch.object(backup, 'load', return_value=cfg):
                backup.export_config(target)
            self.assertEqual(backup.read_backup(target), cfg)
            with patch.object(backup, 'load', return_value=cfg):
                with self.assertRaises(FileExistsError): backup.export_config(target)
    def test_bad_backup_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            path=Path(temp)/'invalid.zip'
            import zipfile
            with zipfile.ZipFile(path,'w') as z:z.writestr('evil.txt','bad')
            with self.assertRaises(ValueError):backup.read_backup(path)
    def test_scheduler_unsupported(self):
        with patch.object(scheduler,'supported',return_value=False):
            self.assertFalse(scheduler.status()['supported'])
            with self.assertRaises(RuntimeError):scheduler.install()
