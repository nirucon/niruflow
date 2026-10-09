"""Read-only, bounded media inventory. No external media parsers required."""
from collections import Counter
from pathlib import Path
import os
import time

EXTENSIONS = {
    'Audio': {'.mp3','.flac','.ogg','.opus','.wav','.m4a','.aac','.wma','.aiff','.alac','.ape'},
    'Video': {'.mp4','.mkv','.mov','.avi','.webm','.m4v','.wmv','.mpeg','.mpg','.ts'},
    'Photos': {'.jpg','.jpeg','.png','.webp','.gif','.tif','.tiff','.bmp','.heic','.heif','.avif','.raw','.cr2','.nef','.arw','.dng'},
}
LOOKUP = {ext:kind for kind,extensions in EXTENSIONS.items() for ext in extensions}
EXCLUDED = {'.git','node_modules','.venv','venv','__pycache__','.cache','.next','.local','.config','.npm','backups'}

def default_roots():
    return [str(Path.home()/name) for name in ('Music','Videos','Pictures')]

def inventory(roots, max_files=1_000_000):
    """Walk real directories only; avoid symlink loops, hidden caches and duplicate roots."""
    started=time.monotonic()
    stats={kind:{'count':0,'bytes':0,'extensions':Counter()} for kind in EXTENSIONS}
    seen=set(); errors=0; total=0; size=0
    for raw in roots:
        root=Path(raw).expanduser()
        if not root.is_dir() or root.is_symlink():
            errors+=1;continue
        root=root.resolve()
        if any(root==prior or prior in root.parents for prior in seen):continue
        seen.add(root)
        def onerror(_):
            nonlocal errors
            errors+=1
        for directory, dirs, files in os.walk(root,followlinks=False,onerror=onerror):
            dirs[:]=[name for name in dirs if name not in EXCLUDED and not name.startswith('.') and not (Path(directory)/name).is_symlink()]
            for name in files:
                kind=LOOKUP.get(Path(name).suffix.lower())
                if not kind:continue
                path=Path(directory)/name
                try:
                    if path.is_symlink():continue
                    st=path.stat()
                    if not os.path.isfile(path):continue
                except OSError:
                    errors+=1;continue
                stats[kind]['count']+=1;stats[kind]['bytes']+=st.st_size
                stats[kind]['extensions'][path.suffix.lower()]+=1
                total+=1;size+=st.st_size
                if total>=max_files:
                    return {'types':stats,'total':total,'bytes':size,'errors':errors,'duration_s':round(time.monotonic()-started,2),'truncated':True}
    return {'types':stats,'total':total,'bytes':size,'errors':errors,'duration_s':round(time.monotonic()-started,2),'truncated':False}
