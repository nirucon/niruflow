import unittest
from niruflow.modules.projects import classify
from niruflow.desktop.themes import THEMES, stylesheet

class V020Tests(unittest.TestCase):
    def test_patch_not_release(self):
        x=classify('lokai-0.1.1-patch-from-0.0.30.zip')
        self.assertNotEqual(x['kind'],'release')
    def test_patch_chain(self):
        x=classify('ingwaz-ai-orkester-patch-0.4.0-to-0.5.0.zip')
        self.assertEqual((x['kind'],x['from'],x['to']),('patch','0.4.0','0.5.0'))
    def test_prerelease(self):
        self.assertEqual(classify('nirupres-2.0.0-rc.2.zip')['kind'],'prerelease')
    def test_unknown_protected(self):
        self.assertEqual(classify('ordning-backup-2026-10-08.zip')['kind'],'unknown')
    def test_themes(self):
        self.assertEqual(set(THEMES),{'Niru Noir','Satie','C. Larsson'})
        self.assertIn('QPushButton',stylesheet('Niru Noir'))
