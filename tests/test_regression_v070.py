from pathlib import Path
from niruflow.modules.media import inventory

def test_media_counts(tmp_path):
    (tmp_path/'song.FLAC').write_bytes(b'abc')
    (tmp_path/'photo.jpg').write_bytes(b'abcde')
    (tmp_path/'video.mp4').write_bytes(b'ab')
    (tmp_path/'ignored.txt').write_bytes(b'abcdef')
    result=inventory([str(tmp_path)])
    assert result['total']==3
    assert result['bytes']==10
    assert [result['types'][k]['count'] for k in ('Audio','Video','Photos')]==[1,1,1]

def test_media_skips_symlink_and_git(tmp_path):
    (tmp_path/'.git').mkdir()
    (tmp_path/'.git'/'hidden.mp3').write_bytes(b'abc')
    (tmp_path/'real.mp3').write_bytes(b'ab')
    (tmp_path/'link.mp3').symlink_to(tmp_path/'real.mp3')
    assert inventory([str(tmp_path)])['total']==1
