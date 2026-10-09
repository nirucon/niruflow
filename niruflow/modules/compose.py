"""Discover Compose manifests without executing them."""
from pathlib import Path
from ..core.locations import expand_path
NAMES=('compose.yaml','compose.yml','docker-compose.yaml','docker-compose.yml')
SKIP={'.git','node_modules','.venv','venv','__pycache__','.next','.cache','vendor','backups','backup','data','volumes','db','database','patches','patch','payload','archive','archives','old'}

def discover(roots,max_depth=4,limit=300):
    found=[]
    for root_value in roots:
        root=expand_path(str(root_value))
        if not root.is_dir() or root.is_symlink(): continue
        stack=[(root,0)]
        while stack and len(found)<limit:
            folder,depth=stack.pop()
            try:
                for name in NAMES:
                    candidate=folder/name
                    if candidate.is_file() and not candidate.is_symlink():
                        found.append({'project':folder.name,'path':str(candidate),'root':str(root)})
                if depth<max_depth:
                    stack.extend((p,depth+1) for p in folder.iterdir() if p.is_dir() and not p.is_symlink() and p.name not in SKIP)
            except (OSError,PermissionError): continue
    return sorted(found,key=lambda x:x['path'])
