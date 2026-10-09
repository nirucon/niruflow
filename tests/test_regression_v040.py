import unittest
from unittest.mock import patch
from niruflow.core.locations import migrate, machine_profile_name
from niruflow.modules import docker

class V040Tests(unittest.TestCase):
    def test_default_only_downloads(self):
        cfg=migrate({'rules':[],'profiles':{'default':{'locations':[{'id':'a','path':'~/Docker','kind':'docker'}]}}})
        self.assertEqual([x['name'] for x in cfg['profiles']['default']['locations']],['Downloads'])
        self.assertEqual(cfg['profiles'][machine_profile_name()]['locations'][0]['path'],'~/Docker')
        self.assertEqual(migrate(cfg),cfg)
    def test_docker_no_shell_mutation(self):
        with patch.object(docker,'_run',return_value='ok') as runner:
            self.assertEqual(docker.container_action('abcdef123456','stop'),'ok')
            runner.assert_called_once_with('container','stop','abcdef123456',timeout=70)
    def test_docker_delete_rejected(self):
        with self.assertRaises(ValueError):docker.container_action('abcdef123456','rm')
    def test_docker_short_id_validation(self):
        with self.assertRaises(ValueError):docker.container_action('x','stop')
    def test_docker_inspection_parses(self):
        with patch.object(docker,'_run',side_effect=['default','{"ID":"abc","Names":"app","State":"running","Status":"Up"}\n','{"Repository":"alpine"}\n','TYPE  TOTAL']):
            data=docker.inspect()
            self.assertEqual(data['summary']['containers'],1)
            self.assertEqual(data['summary']['running'],1)
