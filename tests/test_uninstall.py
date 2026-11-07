"""0.6.1: uninstall.sh removes the program, keeps rules and history unless --purge."""
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FILES = ('.local/bin/niruflow', '.local/share/applications/niruflow.desktop',
         '.local/share/niruflow/versions/0.6.1/venv/bin/python',
         '.config/niruflow/rules.json', '.local/state/niruflow/quarantine/a.txt')


class UninstallTests(unittest.TestCase):
    def run_uninstall(self, home, *args):
        env = {k: v for k, v in os.environ.items() if not k.startswith('XDG_')}
        env.update(HOME=str(home), PATH='/usr/bin:/bin')
        return subprocess.run(['bash', str(ROOT / 'uninstall.sh'), *args], env=env,
                              capture_output=True, text=True, check=False)

    def make(self, home):
        for rel in FILES:
            p = Path(home, rel); p.parent.mkdir(parents=True, exist_ok=True); p.write_text('x')

    def test_default_keeps_rules_and_quarantine(self):
        with tempfile.TemporaryDirectory() as home:
            self.make(home)
            self.assertEqual(self.run_uninstall(home).returncode, 0)
            self.assertFalse(Path(home, '.local/bin/niruflow').exists())
            self.assertFalse(Path(home, '.local/share/niruflow').exists())
            self.assertTrue(Path(home, '.config/niruflow/rules.json').exists())
            self.assertTrue(Path(home, '.local/state/niruflow/quarantine/a.txt').exists())

    def test_purge_removes_everything(self):
        with tempfile.TemporaryDirectory() as home:
            self.make(home)
            self.assertEqual(self.run_uninstall(home, '--purge').returncode, 0)
            for rel in ('.config/niruflow', '.local/state/niruflow', '.local/share/niruflow'):
                self.assertFalse(Path(home, rel).exists(), rel)

    def test_installer_checks_venv_support(self):
        text = (ROOT / 'install.sh').read_text()
        self.assertIn('import ensurepip', text)
        self.assertIn('NIRUX_ASSUME_YES', text)


if __name__ == '__main__':
    unittest.main()
