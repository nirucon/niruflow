"""Pure read-only automation planning; never executes rules or modifies files."""
from pathlib import Path
from .safety import is_nextcloud

def preview_rules(config):
    results=[]
    for rule in config.get('rules',[]):
        target=Path(str(rule.get('path',''))).expanduser()
        enabled=bool(rule.get('enabled',False))
        automatic=bool(rule.get('automatic',False))
        synced=is_nextcloud(target)
        if not enabled: state='disabled'
        elif synced: state='blocked: synced location'
        elif not target.is_dir(): state='unavailable directory'
        elif not automatic: state='manual only'
        else: state='eligible for scheduler'
        results.append({'name':str(rule.get('name','Unnamed rule')),'path':str(target),'status':state,'enabled':enabled})
    return results
