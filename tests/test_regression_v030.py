import tempfile
import unittest
from pathlib import Path
from niruflow.core.locations import migrate, add_location, location_paths
from niruflow.modules.compose import discover
from niruflow.modules.storage import duplicates, largest

class PortableTests(unittest.TestCase):
    def test_migration_preserves_rules(self):
        c={'rules':[{'id':'a','path':'~/Downloads'}],'project_path':'~/Code'}
        migrate(c)
        self.assertEqual(c['rules'][0]['id'],'a')
        self.assertIn('~/Code',[str(p['path']) for p in c['profiles'][c['active_profile']]['locations']])
    def test_profile_and_paths(self):
        c=migrate({'rules':[]})
        add_location(c,'Test','projects','~/Test')
        self.assertTrue(any(p.name=='Test' for p in location_paths(c,'projects')))
    def test_compose_discovery(self):
        with tempfile.TemporaryDirectory() as tmp:
            d=Path(tmp)/'app';d.mkdir();(d/'compose.yaml').write_text('services: {}')
            self.assertEqual(len(discover([tmp])),1)
    def test_duplicates_hash(self):
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp)/'a.txt').write_text('same')
            (Path(tmp)/'b.txt').write_text('same')
            (Path(tmp)/'c.txt').write_text('different')
            self.assertEqual(len(duplicates([tmp])),1)
            self.assertEqual(len(largest([tmp])),3)
    def test_no_symlink_traversal(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)/'root';root.mkdir()
            (root/'link').symlink_to(Path(tmp),target_is_directory=True)
            self.assertEqual(largest([root]),[])
if __name__=='__main__':unittest.main()

class ProjectRootsTests(unittest.TestCase):
    def test_root_level_web_zip(self):
        from niruflow.modules.projects import analyze
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp)/'site-1.2.3.zip').write_bytes(b'zip')
            rows=analyze(tmp)
            self.assertEqual(len(rows),1)
            self.assertEqual(rows[0]['version'],'1.2.3')
