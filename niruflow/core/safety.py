from pathlib import Path
import os
import stat

def is_nextcloud(path):
    return "Nextcloud" in Path(path).expanduser().resolve(strict=False).parts

def safe_file(path,root):
    path=Path(path)
    root=Path(root)
    if root.is_symlink() or path.is_symlink():
        return False
    try:
        rp=path.resolve(strict=True)
        rr=root.resolve(strict=True)
        if rp.parent != rr:
            return False
        st=os.lstat(path)
        return stat.S_ISREG(st.st_mode) and st.st_nlink==1
    except (OSError,ValueError):
        return False

def validate_root(path):
    root=Path(path).expanduser()
    if root.is_symlink() or not root.is_dir():
        raise ValueError("Root must be an existing real directory (not a symlink)")
    resolved=root.resolve()
    home=Path.home().resolve()
    if resolved == home or resolved == Path("/") or home not in resolved.parents:
        raise ValueError("Only a dedicated directory below HOME is permitted")
    if any(part.startswith(".") for part in resolved.relative_to(home).parts):
        raise ValueError("Hidden/system directories are protected")
    return resolved
