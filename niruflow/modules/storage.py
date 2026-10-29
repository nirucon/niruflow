"""Read-only storage inventory, bounded traversal."""
import hashlib
import os
import time
from pathlib import Path
from ..core.locations import expand_path
SKIP={'.git','node_modules','.venv','venv','__pycache__','.cache','.local','.config','.next'}

def fingerprint(path):
    """A bounded content fingerprint for change detection before deletion.

    Small files are fully hashed; large files use the first and last 128 KiB
    plus their size. Metadata checks remain the fast path, and the fingerprint
    catches same-size rewrites when the filesystem timestamp is too coarse.
    """
    import hashlib
    size = Path(path).stat().st_size
    digest = hashlib.sha256()
    digest.update(str(size).encode('ascii'))
    with open(path, 'rb') as stream:
        if size <= 1024 * 1024:
            for chunk in iter(lambda: stream.read(128 * 1024), b''):
                digest.update(chunk)
        else:
            digest.update(stream.read(128 * 1024))
            stream.seek(-128 * 1024, 2)
            digest.update(stream.read(128 * 1024))
    return digest.hexdigest()


def files(root,limit=20000,cancelled=None,progress=None,fingerprint_content=False):
    base=expand_path(str(root))
    if not base.is_dir(): return []
    out=[]; stack=[base]; last_report=0.0
    while stack and len(out)<limit:
        if cancelled and cancelled():break
        folder=stack.pop()
        now=time.monotonic()
        if progress and now-last_report>=0.25:
            progress((len(out),str(folder)))
            last_report=now
        try:
            with os.scandir(folder) as entries:
                for e in entries:
                    if cancelled and cancelled():break
                    if e.is_symlink(): continue
                    try:
                        if e.is_dir(follow_symlinks=False):
                            if e.name not in SKIP: stack.append(Path(e.path))
                        elif e.is_file(follow_symlinks=False):
                            st=e.stat(follow_symlinks=False)
                            if st.st_nlink==1:
                                row={'path':e.path,'bytes':st.st_size,'mtime':st.st_mtime,'ctime_ns':st.st_ctime_ns,'inode':st.st_ino,'device':st.st_dev}
                                if fingerprint_content:
                                    try: row['fingerprint']=fingerprint(e.path)
                                    except OSError: continue
                                out.append(row)
                    except OSError:continue
                    if len(out)>=limit:break
        except OSError:continue
    return out

def largest(roots,limit=100,cancelled=None,progress=None):
    rows=[]
    for root in roots:
        if cancelled and cancelled():break
        rows.extend(files(root,cancelled=cancelled,progress=progress,fingerprint_content=True))
    return sorted(rows,key=lambda x:x['bytes'],reverse=True)[:limit]

def duplicates(roots,limit=100,cancelled=None,progress=None):
    """Group by size, then SHA-256. Never delete or assume which copy is authoritative."""
    by_size={}
    for root in roots:
        if cancelled and cancelled():break
        for row in files(root,cancelled=cancelled,progress=progress):
            if row['bytes']>0:by_size.setdefault(row['bytes'],[]).append(row)
    results=[]
    for size,group in by_size.items():
        if cancelled and cancelled():break
        if len(group)<2:continue
        hashes={}
        for row in group:
            if cancelled and cancelled():break
            try:
                digest=hashlib.sha256()
                with open(row['path'],'rb') as stream:
                    for chunk in iter(lambda:stream.read(1024*1024),b''):
                        if cancelled and cancelled():break
                        digest.update(chunk)
                if cancelled and cancelled():break
                hashes.setdefault(digest.hexdigest(),[]).append(row)
            except OSError:continue
        for digest,items in hashes.items():
            if len(items)>1:
                results.append({'hash':digest,'size':size,'paths':[x['path'] for x in items]})
                if len(results)>=limit:return results
    return results


def file_category(path):
    ext=Path(path).suffix.lower()
    if ext in {'.zip','.7z','.rar','.tar','.gz','.xz','.bz2','.zst'}:return 'Archives'
    if ext in {'.mp4','.m4v','.mkv','.mov','.avi','.webm','.wmv'}:return 'Video'
    if ext in {'.mp3','.flac','.wav','.ogg','.opus','.m4a','.aac'}:return 'Audio'
    if ext in {'.jpg','.jpeg','.png','.gif','.webp','.tif','.tiff','.heic','.psd','.svg'}:return 'Images'
    if ext in {'.pdf','.doc','.docx','.txt','.md','.odt','.xls','.xlsx','.ppt','.pptx'}:return 'Documents'
    return 'Other'


def _identity_changed(current, row):
    """Reject a file whose inode, device or creation-time identity changed."""
    for key, value in (('device', current.st_dev), ('inode', current.st_ino), ('ctime_ns', current.st_ctime_ns)):
        expected = row.get(key)
        if expected is not None and int(expected) != int(value):
            return True
    return False


def storage_quarantine(items,allow_synced=False):
    """Explicitly selected, stat-verified files only; never delete directly."""
    from ..modules.cleanup import quarantine
    from ..core.safety import is_nextcloud, safe_file, validate_root
    moved=[];skipped=[]
    for row in items:
        src=Path(row['path'])
        try:
            root=validate_root(src.parent)
            if is_nextcloud(src) and not allow_synced:raise PermissionError('Synced file requires confirmation')
            if not safe_file(src,root):raise ValueError('Unsafe file')
            st=src.stat()
            if _identity_changed(st,row) or st.st_size!=row['bytes'] or st.st_mtime!=row['mtime']:
                raise ValueError('File changed since scan')
            if row.get('fingerprint') is not None and fingerprint(src) != row['fingerprint']:
                raise ValueError('File content changed since scan')
            candidate={'path':str(src),'bytes':st.st_size,'mtime':st.st_mtime,
                       'ctime':st.st_ctime,'inode':st.st_ino,'device':st.st_dev}
            moved.extend(quarantine({'id':'storage-manual','path':str(root)},[candidate],allow_synced=allow_synced))
        except (OSError,ValueError,PermissionError) as exc:
            skipped.append({'path':str(src),'reason':str(exc)})
    return {'moved':moved,'skipped':skipped}


def storage_delete(items, allow_synced=False):
    """Permanently unlink explicitly selected, unchanged regular files only.

    No directories, symlinks or hardlinked files. Audit every successful unlink.
    """
    import stat
    from ..core.safety import is_nextcloud, safe_file, validate_root
    from ..core.store import record
    deleted, skipped = [], []
    for row in items:
        src = Path(row['path'])
        try:
            root = validate_root(src.parent)
            if is_nextcloud(src) and not allow_synced:
                raise PermissionError('Synced file requires explicit confirmation')
            if not safe_file(src, root):
                raise ValueError('Unsafe file')
            before = os.lstat(src)
            if (not stat.S_ISREG(before.st_mode) or before.st_nlink != 1
                or _identity_changed(before,row)
                or before.st_size != row['bytes'] or abs(before.st_mtime - row['mtime']) > 0.000001):
                raise ValueError('File changed since scan')
            if row.get('fingerprint') is not None and fingerprint(src) != row['fingerprint']:
                raise ValueError('File content changed since scan')
            # Open without following symlinks and verify the inode before unlink.
            flags = os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_CLOEXEC', 0)
            fd = os.open(src, flags)
            try:
                opened = os.fstat(fd)
                now = os.lstat(src)
                if (opened.st_dev, opened.st_ino) != (now.st_dev, now.st_ino) or (now.st_dev, now.st_ino) != (before.st_dev, before.st_ino):
                    raise ValueError('File replaced during deletion')
                if now.st_size != before.st_size or now.st_mtime_ns != before.st_mtime_ns:
                    raise ValueError('File changed during deletion')
                os.unlink(src)
            finally:
                os.close(fd)
            deleted.append(str(src))
            record('storage-manual', str(src), '', 'permanently_deleted', before.st_size,
                   'Explicit GUI confirmation; synced=' + str(is_nextcloud(src)))
        except (OSError, ValueError, PermissionError) as exc:
            skipped.append({'path': str(src), 'reason': str(exc)})
    return {'deleted': deleted, 'skipped': skipped}
