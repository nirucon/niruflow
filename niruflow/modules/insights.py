"""Read-only project maintenance observations. Never infer that a file is safe to delete."""
from collections import Counter
from pathlib import Path

PROTECTED_KINDS={'patch','unknown patch','unknown'}

def project_insights(entries):
    kinds=Counter(x.get('kind','unknown') for x in entries)
    statuses=Counter(x.get('status','protected') for x in entries)
    return {
        'total':len(entries),
        'protected':sum(1 for x in entries if x.get('kind') in PROTECTED_KINDS or x.get('status')=='protected'),
        'review':statuses.get('review',0),
        'kinds':dict(sorted(kinds.items())),
        'message':'Inventory only. Review does not imply safe to delete.',
    }

def protected_location(path):
    """Conservative detection for display; actual deletion rules remain in core.safety."""
    parts={part.casefold() for part in Path(path).expanduser().parts}
    return bool(parts.intersection({'nextcloud','.git','.ssh','.gnupg','.config','venv','.venv','node_modules'}))
