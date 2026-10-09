import os
import time
import uuid
from pathlib import Path
from datetime import datetime,timezone
from ..core.config import STATE
from ..core.safety import validate_root,safe_file,is_nextcloud
from ..core.store import record,history

def scan(rule,now=None):
    root=validate_root(rule["path"])
    age=float(rule.get("hours",24))*3600
    if age<=0:
        raise ValueError("Age must be positive")
    now=time.time() if now is None else now
    found=[]
    for item in root.iterdir():
        if not safe_file(item,root):
            continue
        try:
            st=item.stat()
            # First-run safety: only known stable files; mtime can predate download.
            if now-st.st_mtime >= age and now-st.st_ctime >= age and now-st.st_mtime>=0:
                found.append({"path":str(item),"bytes":st.st_size,"mtime":st.st_mtime,
                              "inode":st.st_ino,"device":st.st_dev,"ctime":st.st_ctime})
        except OSError:
            continue
    return sorted(found,key=lambda x:x["path"])

def quarantine(rule,items,allow_synced=False):
    root=validate_root(rule["path"])
    if is_nextcloud(root) and not allow_synced:
        raise PermissionError("Nextcloud requires explicit --allow-synced confirmation")
    destroot=STATE/"quarantine"
    destroot.mkdir(parents=True,exist_ok=True)
    if os.stat(root).st_dev != os.stat(destroot).st_dev:
        raise OSError("Quarantine must be on same filesystem for atomic moves")
    moved=[]
    for entry in items:
        src=Path(entry["path"])
        if not safe_file(src,root):
            continue
        st=src.stat()
        if any(getattr(st,attr)!=entry[key] for attr,key in
               [("st_ino","inode"),("st_dev","device"),("st_size","bytes"),
                ("st_mtime","mtime"),("st_ctime","ctime")]):
            continue
        target=destroot/(uuid.uuid4().hex+"_"+src.name)
        # Same-filesystem rename, unique destination; never overwrite.
        os.rename(src,target)
        record(rule["id"],src,target,"quarantined",st.st_size)
        moved.append(str(src))
    return moved

def restore(action_id):
    matches=[r for r in history(100000) if r[0]==action_id]
    if not matches:
        raise ValueError("Action not found")
    _,_,rule,source,dest,status,_,_=matches[0]
    if any(row[5]=="restored" and row[7]==f"restored action {action_id}" for row in history(100000)):
        raise ValueError("Already restored")
    if status!="quarantined":
        raise ValueError("Action cannot be restored")
    src=Path(source)
    target=Path(dest)
    if not target.is_file() or target.is_symlink():
        raise FileNotFoundError("Quarantined file missing")
    if src.exists() or src.is_symlink():
        raise FileExistsError("Original path already exists")
    if not src.parent.is_dir():
        raise FileNotFoundError("Original directory missing")
    if os.stat(src.parent).st_dev != os.stat(target).st_dev:
        raise OSError("Restore requires same filesystem")
    # Atomic no-clobber create on the same filesystem; preserve file metadata.
    os.link(target,src,follow_symlinks=False)
    target.unlink()
    record(rule,src,target,"restored",0,f"restored action {action_id}")
    return str(src)
