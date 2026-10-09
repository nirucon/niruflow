"""Portable path profiles; no mutation of target paths."""
import os
from pathlib import Path

KINDS = ('cleanup','projects','docker','storage')

def expand_path(value):
    if not isinstance(value,str) or not value.strip():
        raise ValueError('Sökväg saknas')
    return Path(os.path.expandvars(value)).expanduser().absolute()

def profile(config):
    profiles=config.get('profiles',{})
    active=config.get('active_profile','default')
    return profiles.get(active, {'locations':[]})

def locations(config, kind=None):
    entries=profile(config).get('locations',[])
    return [x for x in entries if kind is None or x.get('kind')==kind]

def location_paths(config, kind):
    return [expand_path(x['path']) for x in locations(config,kind) if x.get('enabled',True)]

def machine_profile_name():
    """Stable per-machine key, without embedding personal paths in shipped defaults."""
    import getpass
    import re
    import socket
    username = re.sub(r'[^a-z0-9-]', '-', getpass.getuser().lower()).strip('-') or 'user'
    hostname = re.sub(r'[^a-z0-9-]', '-', socket.gethostname().lower().split('.')[0]).strip('-') or 'host'
    return f'{username}-{hostname}'


def migrate(config):
    """Idempotently preserve legacy locations in a machine profile.

    The default profile is a template, not a repository of user-specific paths.
    Migration only changes in-memory config until the caller explicitly saves.
    """
    if config.get('profile_schema', 0) >= 3:
        return config
    old_profiles = config.get('profiles') or {}
    old_active = config.get('active_profile', 'default')
    machine = machine_profile_name()
    existing = old_profiles.get('default', {}).get('locations', [])
    if not old_profiles:
        existing = [
            {'id':'projects-dev','name':'Projects-Dev','kind':'projects',
             'path':config.get('project_path','~/Nextcloud/Projects-Dev'),'enabled':True},
            {'id':'projects-web','name':'Projects-web','kind':'projects','path':'~/Nextcloud/Projects-web','enabled':True},
            {'id':'docker','name':'Docker Compose','kind':'docker','path':'~/Docker','enabled':True},
            {'id':'home','name':'Home','kind':'storage','path':'~','enabled':True},
            {'id':'nextcloud','name':'Nextcloud','kind':'storage','path':'~/Nextcloud','enabled':True},
        ]
    template = {'name':'Standard','locations':[
        {'id':'downloads-location','name':'Downloads','kind':'cleanup','path':'~/Downloads','enabled':True}
    ]}
    profiles = dict(old_profiles)
    if existing and machine not in profiles:
        profiles[machine] = {'name':machine,'locations':[dict(x) for x in existing]}
    profiles['default'] = template
    config['profiles'] = profiles
    if old_active == 'default' and existing:
        config['active_profile'] = machine
    else:
        config['active_profile'] = old_active if old_active in profiles else 'default'
    config['profile_schema'] = 3
    config.setdefault('docker_context','')
    return config

def add_location(config,name,kind,path):
    if kind not in KINDS: raise ValueError('Okänd platstyp')
    if not name.strip(): raise ValueError('Namn krävs')
    entry={'id':os.urandom(6).hex(),'name':name.strip(),'kind':kind,'path':path.strip(),'enabled':True}
    expand_path(entry['path'])
    profile(config)['locations'].append(entry)
    return entry
