from pathlib import Path

def test_no_swedish_literals_in_desktop():
    src=(Path(__file__).resolve().parents[1]/'niruflow/desktop/app.py').read_text()
    assert 'Filtrera projekt' not in src
    assert "['Projekt'" not in src
    assert 'Analys pågår' not in src
