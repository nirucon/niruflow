"""Conservative release inventory: unknown artifacts never become deletion candidates."""
import re
from pathlib import Path

SEMVER = re.compile(r'(?<!\d)v?(\d+)\.(\d+)\.(\d+)(?:[-.]?(alpha|beta|rc)[.-]?(\d+)?)?', re.I)
PATCH = re.compile(r'(?:^|[-_])patch[-_](?:from[-_])?v?(\d+\.\d+\.\d+)[-_](?:to[-_])?v?(\d+\.\d+\.\d+)', re.I)


def classify(name):
    stem = name[:-4] if name.lower().endswith('.zip') else name
    match = PATCH.search(stem)
    if match:
        return {'kind':'patch','from':match.group(1),'to':match.group(2), 'version':match.group(2), 'family':stem[:match.start()].strip('-_') or 'unknown'}
    if 'patch' in stem.lower():
        return {'kind':'unknown patch','version':'', 'family':stem, 'from':'', 'to':''}
    matches = list(SEMVER.finditer(stem))
    if len(matches) != 1:
        return {'kind':'unknown','version':'','family':stem, 'from':'','to':''}
    m = matches[0]
    # A suffix after the version may describe a different artifact type.
    tail = stem[m.end():].lower().strip('-_')
    if tail and not any(x in tail for x in ('clean','release','source','full')):
        return {'kind':'unknown','version':'','family':stem, 'from':'','to':''}
    family = stem[:m.start()].strip('-_') or 'unknown'
    kind = 'prerelease' if m.group(4) else 'release'
    return {'kind':kind,'version':m.group(0).lstrip('vV'),'family':family,'from':'','to':''}


def analyze(path, keep=2):
    root = Path(path).expanduser()
    if not root.is_dir():
        return []
    if not 1 <= keep <= 20:
        raise ValueError('keep must be between 1 and 20')
    entries = []
    # Root-level release ZIPs (common in Projects-web) must not be missed.
    for child in root.iterdir():
        if child.is_symlink() or not child.is_file() or child.suffix.lower()!='.zip':
            continue
        data=classify(child.name)
        entries.append({'project':root.name,'name':child.name,'path':str(child),**data,
                        'status':'protected' if data['kind'] not in ('release','prerelease') else 'review'})
    for project in root.iterdir():
        if not project.is_dir() or project.is_symlink():
            continue
        try:
            children = list(project.iterdir())
        except OSError:
            continue
        for child in children:
            if child.is_symlink() or (not child.is_dir() and not (child.is_file() and child.suffix.lower()=='.zip')):
                continue
            data = classify(child.name)
            entries.append({'project':project.name,'name':child.name,'path':str(child), **data,
                            'status':'protected' if data['kind'] not in ('release','prerelease') else 'review'})
    # No automatic 'safe to delete' classification. Keep newest stable AND newest prereleases.
    groups = {}
    for entry in entries:
        if entry['kind'] in ('release','prerelease'):
            groups.setdefault((entry['project'],entry['family'].lower(),entry['kind']),[]).append(entry)
    def version_key(item):
        m = SEMVER.search(item['version'])
        return tuple(int(m.group(i)) for i in (1,2,3)) if m else (0,0,0)
    for group in groups.values():
        versions = sorted({x['version'] for x in group},key=lambda v:version_key({'version':v}),reverse=True)
        for entry in group:
            entry['status']='keep' if entry['version'] in versions[:keep] else 'review'
    return sorted(entries,key=lambda x:(x['project'].lower(),x['family'].lower(),x['name'].lower()))
