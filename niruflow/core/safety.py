from pathlib import Path
import hashlib
import os
import stat

# Known sync roots. The comparison is case-insensitive and path-component
# based, so a folder named "Nextcloud" or "Dropbox" is protected even after a
# case change, while an unrelated filename such as "my-sync-notes" is not.
SYNC_MARKERS = (
    "nextcloud", "owncloud", "dropbox", "onedrive", "google drive",
    "google-drive", "googledrive", "megasync", "mega", "pcloud",
    "icloud drive", "icloud-drive", "seafile", "syncthing", "rclone",
)

def sync_provider(path):
    parts = {part.casefold() for part in Path(path).expanduser().resolve(strict=False).parts}
    for marker in SYNC_MARKERS:
        if marker in parts:
            return marker
    return None

def is_synced(path):
    return sync_provider(path) is not None

def is_nextcloud(path):
    # Kept as the compatibility name used by older callers and tests. It now
    # protects every recognized sync provider, not only Nextcloud.
    return is_synced(path)

def identity(st):
    return {"device": st.st_dev, "inode": st.st_ino, "ctime_ns": st.st_ctime_ns}

def fingerprint(path):
    """Bounded content fingerprint; full SHA-256 for files up to 64 MiB."""
    size = Path(path).stat().st_size
    digest = hashlib.sha256()
    digest.update(str(size).encode("ascii"))
    with open(path, "rb") as stream:
        if size <= 64 * 1024 * 1024:
            for chunk in iter(lambda: stream.read(128 * 1024), b""):
                digest.update(chunk)
        else:
            digest.update(stream.read(128 * 1024))
            stream.seek(-128 * 1024, 2)
            digest.update(stream.read(128 * 1024))
    return digest.hexdigest()

def fingerprint_full(path):
    return Path(path).stat().st_size <= 64 * 1024 * 1024

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
