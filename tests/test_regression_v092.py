import unittest
from unittest.mock import patch
import json
from datetime import datetime,timezone,timedelta
from niruflow.modules.docker_cleanup import preview,apply

A='a'*64
B='b'*64
C='c'*64
OLD=(datetime.now(timezone.utc)-timedelta(days=90)).isoformat()

def fake(*args,**kwargs):
    if args==('context','show'):return 'default'
    if args[:3]==('container','ls','-aq'):return C
    if args[:2]==('container','inspect'):return json.dumps([{'Image':'sha256:'+C}])
    if args[:3]==('image','ls','-aq'):return '\n'.join([A,B,C])
    if args==('image','inspect','project:v1'):
        return json.dumps([{'Id':'sha256:'+A,'RepoTags':['project:v1'],'Created':OLD,'Size':100}])
    if args[:2]==('image','inspect'):
        return json.dumps([{'Id':'sha256:'+i,'RepoTags':[('project:v1' if i==A else '<none>:<none>')], 'Created':OLD,'Size':100} for i in [A,B,C]])
    if args[:2]==('image','rm'):return args[2]
    raise AssertionError(args)

class TestDockerCleanup092(unittest.TestCase):
    @patch('niruflow.modules.docker_cleanup._run',side_effect=fake)
    def test_tagged_opt_in_and_references(self,run):
        result=preview(1,True)
        self.assertEqual({x['id'] for x in result['candidates']},{A,B})
        self.assertEqual(result['excluded']['in_use'],1)
    @patch('niruflow.modules.docker_cleanup._run',side_effect=fake)
    def test_tagged_opt_out(self,run):
        result=preview(1,False)
        self.assertEqual([x['id'] for x in result['candidates']],[B])
        self.assertEqual(result['excluded']['tagged_opt_out'],1)
    @patch('niruflow.modules.docker_cleanup._run',side_effect=fake)
    def test_apply_requires_confirmation(self,run):
        with self.assertRaises(PermissionError):apply(preview(1,True),[A],'yes')
    @patch('niruflow.modules.docker_cleanup._run',side_effect=fake)
    def test_selected_only(self,run):
        result=apply(preview(1,True),[A],'DELETE IMAGES')
        self.assertEqual(result['removed'],[A])
        self.assertEqual(result['errors'],[])
