from pathlib import Path

def test_shared_widgets_module_exists():
    root=Path(__file__).resolve().parents[1]
    text=(root/'niruflow/desktop/widgets.py').read_text()
    assert 'def make_table(' in text
    assert 'setUpdatesEnabled(False)' in text
