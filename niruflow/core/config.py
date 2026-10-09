import json
import os
import shutil
from pathlib import Path
from .locations import migrate

BASE = Path(os.environ.get("XDG_CONFIG_HOME", Path.home()/".config"))/"niruflow"
STATE = Path(os.environ.get("XDG_STATE_HOME", Path.home()/".local/state"))/"niruflow"
CONFIG = BASE/"config.json"

def defaults():
    home = Path.home()
    return migrate({
        "version": 2,
        "language": "en",
        "theme": "Niru Noir",
        "quarantine_days": 30,
        "rules": [
            {"id":"downloads","name":"Downloads","path":str(home/"Downloads"),
             "hours":24,"enabled":False,"recursive":False,"automatic":True},
            {"id":"screenshots","name":"Screenshots","path":str(home/"Nextcloud/Screenshots"),
             "hours":24,"enabled":False,"recursive":False,"automatic":True},
        ],
        "protected_names": ["Important","Fun","SAVE"],
        "project_path": str(home/"Nextcloud/Projects-Dev"),
    })

def load():
    if not CONFIG.exists():
        return defaults()
    try:
        raw = json.loads(CONFIG.read_text(encoding="utf-8"))
        if not isinstance(raw,dict) or not isinstance(raw.get("rules"),list):
            raise ValueError("Invalid configuration")
        before = raw.get('profile_schema', 0)
        updated = migrate(raw)
        if before < 3:
            # Preserve exact legacy bytes before persisting the one-time migration.
            backup = CONFIG.with_name('config.pre-0.4.0.json')
            if not backup.exists():
                shutil.copy2(CONFIG, backup)
            save(updated)
        return updated
    except (OSError, ValueError) as exc:
        raise RuntimeError(f"Cannot load {CONFIG}: {exc}") from exc

def save(config):
    BASE.mkdir(parents=True,exist_ok=True)
    tmp = CONFIG.with_suffix(".tmp")
    with tmp.open("w",encoding="utf-8") as f:
        os.fchmod(f.fileno(),0o600)
        json.dump(config,f,indent=2,ensure_ascii=False)
        f.flush()
        os.fsync(f.fileno())
    if CONFIG.exists():
        backup = CONFIG.with_name("config.previous.json")
        shutil.copy2(CONFIG, backup)
        os.chmod(backup, 0o600)
    os.replace(tmp,CONFIG)
