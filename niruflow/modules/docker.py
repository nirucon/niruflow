"""Docker Engine adapter. No shell=True and no implicit destructive operations."""
import json
import shutil
import subprocess


def _run(*args, timeout=25):
    if not shutil.which('docker'):
        raise RuntimeError('Docker CLI saknas. Installera Docker och kontrollera PATH.')
    try:
        result = subprocess.run(['docker', *args], capture_output=True, text=True,
                                timeout=timeout, check=False)
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError('Docker svarade inte inom tidsgränsen') from exc
    if result.returncode:
        raise RuntimeError((result.stderr or result.stdout).strip() or 'Docker-kommandot misslyckades')
    return result.stdout


def _lines(*args):
    result=[]
    for line in _run(*args).splitlines():
        if line.strip():
            try: result.append(json.loads(line))
            except json.JSONDecodeError as exc:
                raise RuntimeError('Docker gav ogiltigt JSON-svar') from exc
    return result


def inspect():
    context=_run('context','show').strip()
    containers=_lines('ps','-a','--no-trunc','--format','{{json .}}')
    images=_lines('image','ls','-a','--no-trunc','--format','{{json .}}')
    return {'context':context,'containers':containers,'images':images,
            'disk':_run('system','df'),
            'summary':{'containers':len(containers),
                       'running':sum(str(x.get('State','')).lower()=='running' for x in containers),
                       'unhealthy':sum('unhealthy' in str(x.get('Status','')).lower() for x in containers),
                       'images':len(images)},'readonly':True}


def container_action(container_id, action):
    """Execute an explicit, non-deleting operation against an exact container ID."""
    if action not in ('start','stop','restart','pause','unpause'):
        raise ValueError('Otillåten containeråtgärd')
    if not isinstance(container_id,str) or not container_id or any(c not in '0123456789abcdef' for c in container_id.lower()) or len(container_id)<12:
        raise ValueError('Ett giltigt Docker container-ID krävs')
    return _run('container',action,container_id,timeout=70).strip()


def logs(container_id, tail=200):
    if not isinstance(container_id,str) or len(container_id)<12 or any(c not in '0123456789abcdef' for c in container_id.lower()):
        raise ValueError('Ogiltigt container-ID')
    return _run('logs','--tail',str(max(1,min(int(tail),500))),container_id,timeout=30)


def mounts(container_id):
    if not isinstance(container_id,str) or len(container_id)<12 or any(c not in '0123456789abcdef' for c in container_id.lower()):
        raise ValueError('Ogiltigt container-ID')
    data=json.loads(_run('container','inspect',container_id))
    return data[0].get('Mounts',[]) if data else []


def container_remove(container_id, expected_context, confirmation):
    """Remove one explicitly approved stopped container; never remove volumes."""
    import re
    if confirmation != 'REMOVE CONTAINER':
        raise PermissionError('Explicit confirmation required')
    if not isinstance(container_id,str) or not re.fullmatch(r'[a-fA-F0-9]{64}',container_id):
        raise ValueError('A full Docker container ID is required')
    if not expected_context or _run('context','show').strip() != expected_context:
        raise RuntimeError('Docker context changed; refresh first')
    inspected=json.loads(_run('container','inspect',container_id))
    if len(inspected)!=1 or inspected[0].get('Id','').lower()!=container_id.lower():
        raise RuntimeError('Container identity changed')
    if inspected[0].get('State',{}).get('Running',True):
        raise RuntimeError('Container is running; stop it explicitly first')
    if inspected[0].get('State',{}).get('Paused',False):
        raise RuntimeError('Paused containers are protected')
    # No -f and no -v: Docker refuses running containers; named/anonymous volumes remain.
    return _run('container','rm',container_id,timeout=90).strip()
