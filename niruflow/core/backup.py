"""Portable, configuration-only backup. No history, secrets or quarantined files."""
import json
import os
import zipfile
from datetime import datetime, timezone
from pathlib import Path
from .config import load, save

MEMBER = "config.json"

def export_config(destination):
    destination = Path(destination).expanduser()
    if destination.suffix.lower() != ".zip":
        raise ValueError("Backup must be a .zip file")
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists():
        raise FileExistsError(destination)
    config = load()
    payload = json.dumps(config, ensure_ascii=False, indent=2).encode("utf-8")
    if len(payload) > 2_000_000:
        raise ValueError("Configuration is unexpectedly large")
    with zipfile.ZipFile(destination, "x", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("manifest.json", json.dumps({"application":"NIRUFLOW", "format":1, "exported_at":datetime.now(timezone.utc).isoformat()}))
        archive.writestr(MEMBER, payload)
    os.chmod(destination, 0o600)
    return destination

def read_backup(source):
    with zipfile.ZipFile(source) as archive:
        if set(archive.namelist()) != {"manifest.json", MEMBER}:
            raise ValueError("Invalid backup contents")
        for member in archive.infolist():
            if member.file_size > 2_000_000:
                raise ValueError("Backup member too large")
        manifest = json.loads(archive.read("manifest.json"))
        if manifest.get("application") != "NIRUFLOW" or manifest.get("format") != 1:
            raise ValueError("Unsupported backup format")
        cfg = json.loads(archive.read(MEMBER))
    if not isinstance(cfg, dict) or not isinstance(cfg.get("rules"), list) or not isinstance(cfg.get("profiles"), dict):
        raise ValueError("Invalid configuration structure")
    return cfg

def import_config(source):
    from .config import CONFIG
    cfg = read_backup(source)
    if CONFIG.exists():
        snapshot = CONFIG.with_name("config.before-import-" + datetime.now().strftime("%Y%m%d-%H%M%S-%f") + ".json")
        shutil_copy(CONFIG, snapshot)
    save(cfg)
    return cfg

def shutil_copy(src, dest):
    import shutil
    shutil.copy2(src, dest)
    os.chmod(dest, 0o600)
