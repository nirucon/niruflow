import unittest
from unittest.mock import patch
from niruflow.modules.docker import container_remove

class DockerRemovalTests(unittest.TestCase):
    def test_requires_confirmation(self):
        with self.assertRaises(PermissionError):container_remove('a'*64,'default','')
    def test_rejects_short_id(self):
        with self.assertRaises(ValueError):container_remove('abc','default','REMOVE CONTAINER')
    @patch('niruflow.modules.docker._run')
    def test_stopped_container_only(self,run):
        import json
        cid='a'*64
        run.side_effect=['default',json.dumps([{'Id':cid,'State':{'Running':False}}]),cid]
        self.assertEqual(container_remove(cid,'default','REMOVE CONTAINER'),cid)
        self.assertEqual(run.call_args.args,('container','rm',cid))
    @patch('niruflow.modules.docker._run')
    def test_running_protected(self,run):
        import json
        cid='b'*64
        run.side_effect=['default',json.dumps([{'Id':cid,'State':{'Running':True}}])]
        with self.assertRaises(RuntimeError):container_remove(cid,'default','REMOVE CONTAINER')
