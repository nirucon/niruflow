import unittest
from unittest.mock import patch
from niruflow.core.formatting import human_size
from niruflow.modules import docker_cleanup

class TestV060(unittest.TestCase):
    def test_sizes(self):
        self.assertEqual(human_size(1063435190),'1.1 GB')
        self.assertEqual(human_size(541249161),'541.2 MB')
        self.assertEqual(human_size(0),'0 B')
    def test_confirmation(self):
        with self.assertRaises(PermissionError):
            docker_cleanup.apply({'context':'default'},[], 'YES')
    def test_empty_preview(self):
        def fake(*args,**kwargs):
            return 'default' if args[:2]==('context','show') else ''
        with patch.object(docker_cleanup,'_run',side_effect=fake):
            plan=docker_cleanup.preview()
        self.assertEqual(plan['candidates'],[])
        self.assertEqual(plan['context'],'default')
    def test_age_validation(self):
        with self.assertRaises(ValueError):docker_cleanup.preview(0)
