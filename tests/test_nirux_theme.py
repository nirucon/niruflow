import tempfile
import unittest
from pathlib import Path

from niruflow.desktop.themes import FOLLOW_NIRUX, nirux_palette, theme_names


class NiruxThemeTests(unittest.TestCase):
    def test_nirux_follows_desktop_theme(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory) / 'niruwm'
            (base / 'themes').mkdir(parents=True)
            (base / 'config.toml').write_text('theme = "niru-nord"\n')
            palette = nirux_palette(config_dir=Path(directory))
            self.assertEqual(palette[0], '#2e3440')
            self.assertEqual(palette[2], '#eceff4')

    def test_nirux_custom_theme_file(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory) / 'niruwm'
            themes = base / 'themes'
            themes.mkdir(parents=True)
            (base / 'config.toml').write_text('theme = "custom"\n')
            (themes / 'custom.toml').write_text('background = "abcdef"\nforeground = "123456"\n')
            palette = nirux_palette(config_dir=Path(directory))
            self.assertEqual(palette[0], '#abcdef')
            self.assertEqual(palette[2], '#123456')

    def test_follow_is_offered(self):
        self.assertIn(FOLLOW_NIRUX, theme_names())
