import json
import unittest
from unittest.mock import patch
from datetime import datetime,timezone,timedelta
from niruflow.modules.docker_cleanup import preview,apply

ID='d'*64
CREATED=(datetime.now(timezone.utc)-timedelta(days=90)).isoformat()

def docker(*args,**kwargs):
    if args==('context','show'):return 'default'
    if args[:3]==('container','ls','-aq'):return ''
    if args[:3]==('image','ls','-aq'):return ID
    if args[:2]==('image','inspect'):
        return json.dumps([{'Id':'sha256:'+ID,'RepoTags':['app:1','mirror:1'],'Created':CREATED,'Size':123}])
    if args[:2]==('image','rm'):
        if args[2]==ID:raise RuntimeError('must be forced - multiple repositories')
        return args[2]
    raise AssertionError(args)

class TestTaggedImages(unittest.TestCase):
    @patch('niruflow.modules.docker_cleanup._run',side_effect=docker)
    def test_removes_tags_not_id(self,run):
        plan=preview(7,True)
        result=apply(plan,[ID],'DELETE IMAGES')
        self.assertEqual(result['removed'],[ID])
        self.assertEqual(result['removed_tags'],2)
        self.assertEqual(result['errors'],[])
        self.assertNotIn(('image','rm',ID),[c.args for c in run.call_args_list])
