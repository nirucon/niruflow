import os
import tempfile
import unittest
from pathlib import Path

from niruflow.core.safety import fingerprint, fingerprint_full, sync_provider
from niruflow.desktop.themes import FOLLOW_NIRUX, nirux_theme_signature, theme_names
from niruflow.modules.forgotten import find_old_files


class SyncTests(unittest.TestCase):
    def test_known_sync_roots_are_protected_case_insensitively(self):
        for path, provider in [
            ('~/Nextcloud/file', 'nextcloud'),
            ('~/Dropbox/file', 'dropbox'),
            ('~/OneDrive/file', 'onedrive'),
            ('~/Google Drive/file', 'google drive'),
            ('~/MEGAsync/file', 'megasync'),
            ('~/rclone/file', 'rclone'),
        ]:
            self.assertEqual(sync_provider(path), provider)

    def test_unrelated_names_are_not_sync_roots(self):
        self.assertIsNone(sync_provider('~/projects/my-sync-notes/file.txt'))


class FingerprintTests(unittest.TestCase):
    def test_small_files_use_a_full_fingerprint(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'small'
            path.write_bytes(b'abc')
            self.assertTrue(fingerprint_full(path))
            self.assertEqual(fingerprint(path), fingerprint(path))

    def test_large_files_are_explicitly_bounded(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'large'
            with path.open('wb') as stream:
                stream.truncate(65 * 1024 * 1024)
            self.assertFalse(fingerprint_full(path))
            self.assertEqual(len(fingerprint(path)), 64)

    def test_old_file_rows_carry_deletion_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'old.txt'
            path.write_text('old')
            old = 1_600_000_000
            os.utime(path, (old, old))
            rows = find_old_files(directory, months=1)
            self.assertEqual(len(rows), 1)
            row = rows[0]
            for key in ('device', 'inode', 'ctime_ns', 'fingerprint', 'fingerprint_full', 'mtime'):
                self.assertIn(key, row)


class ThemeWatchTests(unittest.TestCase):
    def test_signature_changes_with_the_desktop_theme(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory) / 'niruwm'
            themes = base / 'themes'
            themes.mkdir(parents=True)
            (base / 'config.toml').write_text('theme = "niru-noir"\n')
            first = nirux_theme_signature(Path(directory))
            (base / 'config.toml').write_text('theme = "niru-nord"\n')
            second = nirux_theme_signature(Path(directory))
            self.assertNotEqual(first, second)
            self.assertIn(FOLLOW_NIRUX, theme_names())
