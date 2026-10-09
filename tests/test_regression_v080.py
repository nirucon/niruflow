from pathlib import Path
import os
from unittest.mock import patch
from niruflow.modules.storage import file_category, storage_quarantine
from niruflow.modules.compose import discover


def test_categories():
    assert file_category('song.flac') == 'Audio'
    assert file_category('movie.mkv') == 'Video'
    assert file_category('archive.zip') == 'Archives'
    assert file_category('cover.png') == 'Images'


def test_storage_quarantine_and_restore(tmp_path):
    from niruflow.modules.cleanup import restore
    from niruflow.core.store import history
    home=tmp_path/'home';root=home/'Downloads';root.mkdir(parents=True)
    state=home/'.local'/'state'/'niruflow';state.mkdir(parents=True)
    src=root/'example.zip';src.write_bytes(b'example')
    st=src.stat()
    with patch('pathlib.Path.home',return_value=home), patch('niruflow.modules.cleanup.STATE',state), patch('niruflow.core.store.STATE',state):
        result=storage_quarantine([{'path':str(src),'bytes':st.st_size,'mtime':st.st_mtime}])
        assert result['moved']==[str(src)]
        assert not src.exists()
        # restore using real store, without affecting user's configuration
        actions=history(20)
        assert any(row[3]==str(src) for row in actions)
        action=next(row for row in actions if row[3]==str(src))
        restore(action[0])
        assert src.read_bytes()==b'example'


def test_changed_file_skipped(tmp_path):
    home=tmp_path/'home';root=home/'Downloads';root.mkdir(parents=True)
    src=root/'test.txt';src.write_text('first');st=src.stat()
    src.write_text('changed content')
    with patch('pathlib.Path.home',return_value=home):
        result=storage_quarantine([{'path':str(src),'bytes':st.st_size,'mtime':st.st_mtime}])
    assert not result['moved'] and src.exists()


def test_compose_ignores_patch_payload(tmp_path):
    project=tmp_path/'project';project.mkdir()
    (project/'compose.yaml').write_text('services: {}')
    patchdir=project/'patches'/'0.1'/'payload';patchdir.mkdir(parents=True)
    (patchdir/'compose.yaml').write_text('services: {}')
    assert len(discover([str(tmp_path)]))==1
