"""Conservative Docker image cleanup: explicit preview and revalidation.

Never prunes volumes, networks, containers or tagged images without an
explicit opt-in. Uses the current Docker context. Not a disk-savings estimate.
"""
from datetime import datetime, timezone, timedelta
from .docker import _run
import json
import re

SHA = re.compile(r"^[a-f0-9]{64}$")

def _json_command(*args):
    return json.loads(_run(*args, timeout=90))

def preview(min_age_days=30, include_unused=False):
    if not 1 <= int(min_age_days) <= 3650:
        raise ValueError('Age must be between 1 and 3650 days')
    context = _run('context', 'show').strip()
    ids = _run('container', 'ls', '-aq', '--no-trunc').split()
    referenced = set()
    if ids:
        # inspect is authoritative even for stopped containers
        for entry in _json_command('container', 'inspect', *ids):
            referenced.add(str(entry.get('Image', '')).removeprefix('sha256:'))
    image_ids = list(dict.fromkeys(_run('image', 'ls', '-aq', '--no-trunc').split()))
    candidates=[]
    excluded={'in_use':0,'tagged_opt_out':0,'too_new':0,'invalid':0}
    cutoff = datetime.now(timezone.utc) - timedelta(days=int(min_age_days))
    if image_ids:
        # Avoid excessive CLI argument length and large output
        for offset in range(0,len(image_ids),40):
            for entry in _json_command('image','inspect',*image_ids[offset:offset+40]):
                image_id = str(entry.get('Id','')).removeprefix('sha256:')
                if not SHA.fullmatch(image_id):
                    excluded['invalid'] += 1
                    continue
                if image_id in referenced:
                    excluded['in_use'] += 1
                    continue
                tags = [t for t in entry.get('RepoTags',[]) if t and t != '<none>:<none>']
                if tags and not include_unused:
                    excluded['tagged_opt_out'] += 1
                    continue
                try:
                    created = datetime.fromisoformat(entry['Created'].replace('Z','+00:00'))
                except (ValueError,KeyError,TypeError):
                    excluded['invalid'] += 1
                    continue
                if created > cutoff:
                    excluded['too_new'] += 1
                    continue
                candidates.append({'id':image_id,'tags':tags,'created':created.isoformat(),
                                   'bytes':int(entry.get('Size',0)),'dangling':not tags})
    return {'context':context,'age_days':int(min_age_days),'include_unused':bool(include_unused),
            'candidates':candidates,'count':len(candidates), 'excluded':excluded,
            'apparent_bytes':sum(x['bytes'] for x in candidates)}

def apply(preview_data, approved_ids, confirmation):
    """Re-preview against current engine/context; remove only exact unchanged candidates."""
    if confirmation != 'DELETE IMAGES':
        raise PermissionError('Type DELETE IMAGES to confirm')
    if _run('context','show').strip() != preview_data['context']:
        raise RuntimeError('Docker context changed; preview again')
    current = preview(preview_data['age_days'],preview_data['include_unused'])
    allowed = {x['id']:x for x in current['candidates']}
    previous = {x['id']:x for x in preview_data['candidates']}
    approved = list(dict.fromkeys(approved_ids))
    if any(not isinstance(i, str) or not SHA.fullmatch(i) for i in approved):
        raise ValueError('Invalid Docker image ID')
    if not approved or any(i not in allowed or i not in previous or allowed[i] != previous[i] for i in approved):
        raise RuntimeError('Candidates changed; preview again')
    removed=[]; errors=[]; removed_tags=0
    for image_id in approved:
        try:
            # no force: Docker refuses removal of images still in use
            # Check container references again immediately before each removal.
            live_ids = _run('container','ls','-aq','--no-trunc').split()
            for offset in range(0,len(live_ids),40):
                for container in _json_command('container','inspect',*live_ids[offset:offset+40]):
                    if str(container.get('Image','')).removeprefix('sha256:') == image_id:
                        raise RuntimeError('Image is now referenced by a container')
            # Docker refuses removal by ID if multiple repository tags refer to it.
            # Remove each explicitly previewed tag, never use --force.
            tags = previous[image_id]['tags']
            if tags:
                for tag in tags:
                    # Revalidate tag ownership and container references immediately.
                    if _run('context','show').strip() != preview_data['context']:
                        raise RuntimeError('Docker context changed during cleanup')
                    live_ids = _run('container','ls','-aq','--no-trunc').split()
                    for offset in range(0,len(live_ids),40):
                        for container in _json_command('container','inspect',*live_ids[offset:offset+40]):
                            if str(container.get('Image','')).removeprefix('sha256:') == image_id:
                                raise RuntimeError('Image is now referenced by a container')
                    live_image = _json_command('image','inspect',tag)
                    if len(live_image) != 1 or str(live_image[0].get('Id','')).removeprefix('sha256:') != image_id:
                        raise RuntimeError(f'Tag changed since preview: {tag}')
                    _run('image','rm',tag,timeout=90)
                    removed_tags += 1
            else:
                _run('image','rm',image_id,timeout=90)
            removed.append(image_id)
        except RuntimeError as exc:
            errors.append({'id':image_id,'error':str(exc)})
    return {'removed':removed,'errors':errors,'removed_tags':removed_tags}
