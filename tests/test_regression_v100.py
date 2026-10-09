import unittest
from pathlib import Path
from niruflow.core.version import VERSION
from niruflow.modules.docker_cleanup import apply
from unittest.mock import patch

class ReleaseTests(unittest.TestCase):
    def test_version(self):
        self.assertEqual(VERSION, '0.4.1')
    def test_docker_rejects_invalid_ids(self):
        data={'context':'default','age_days':7,'include_unused':False,'candidates':[]}
        with patch('niruflow.modules.docker_cleanup._run', return_value='default'), patch('niruflow.modules.docker_cleanup.preview', return_value={'candidates':[]}):
            with self.assertRaises(ValueError):
                apply(data,['not-an-image-id'],'DELETE IMAGES')
    def test_readme_documents_synced_deletion(self):
        self.assertIn('synchronize', Path('README.md').read_text())
