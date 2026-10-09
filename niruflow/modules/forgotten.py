"""Conservative, read-only inspection of long-unchanged files."""
import os
import stat
import time
from datetime import datetime, timezone
from pathlib import Path

EXCLUDED = {".cache", ".local", ".config", ".git", ".ssh", ".gnupg", ".mozilla", ".thunderbird", ".var", "node_modules", "venv", ".venv", "proc", "sys"}

def find_old_files(root, months=12, limit=100, cancelled=None):
    if not 1 <= months <= 120 or not 1 <= limit <= 10000:
        raise ValueError("Invalid age or limit")
    root=Path(root).expanduser()
    if not root.is_dir() or root.is_symlink():
        raise ValueError("Invalid root")
    threshold=time.time() - months * 30.4375 * 86400
    found=[]
    def onerror(_error):
        pass
    for directory,dirs,files in os.walk(root,followlinks=False,onerror=onerror):
        if cancelled and cancelled():
            return sorted(found,key=lambda x:x["modified"])
        dirs[:]=[d for d in dirs if d not in EXCLUDED and not d.startswith('.') and not (Path(directory)/d).is_symlink()]
        for name in files:
            if name.startswith('.') or name.endswith(('.part','.crdownload','.tmp','.lock')):
                continue
            path=Path(directory)/name
            try:
                st=path.lstat()
                if not stat.S_ISREG(st.st_mode) or st.st_nlink!=1 or st.st_mtime>threshold:
                    continue
                found.append({"path":str(path),"bytes":st.st_size,"modified":datetime.fromtimestamp(st.st_mtime,timezone.utc).isoformat()})
                if len(found)>=limit:
                    return sorted(found,key=lambda x:x["modified"])
            except OSError:
                continue
    return sorted(found,key=lambda x:x["modified"])
