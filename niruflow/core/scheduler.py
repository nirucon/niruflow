"""Manage optional systemd user timer without touching the GUI lifecycle."""
import shutil
import subprocess
from pathlib import Path

UNIT = "niruflow.timer"

def supported():
    return shutil.which("systemctl") is not None and Path("/run/systemd/system").exists()

def _call(*args):
    if not supported():
        raise RuntimeError("Systemd user services are unavailable on this machine")
    return subprocess.run(["systemctl", "--user", *args], text=True, capture_output=True, timeout=12)

def status():
    if not supported():
        return {"supported":False,"installed":False,"enabled":False,"active":False,"detail":"Systemd user timer unavailable (e.g. Void/runit)"}
    path = Path.home()/".config/systemd/user"/UNIT
    enabled = _call("is-enabled", UNIT)
    active = _call("is-active", UNIT)
    return {"supported":True,"installed":path.exists(),"enabled":enabled.returncode==0,"active":active.returncode==0,"detail":active.stdout.strip() or active.stderr.strip()}

def install():
    if not supported():
        raise RuntimeError("This scheduler requires systemd; CLI run-enabled works on other systems")
    import shutil as _shutil
    exe = _shutil.which("niruflow") or str(Path.home()/".local/bin/niruflow")
    if not Path(exe).exists():
        raise RuntimeError("Install NIRUFLOW first, so the CLI executable exists")
    directory=Path.home()/".config/systemd/user"
    directory.mkdir(parents=True,exist_ok=True)
    service=directory/"niruflow.service"
    timer=directory/UNIT
    if service.exists() or timer.exists():
        raise FileExistsError("Scheduler units already exist; inspect them before replacing")
    service.write_text("[Unit]\nDescription=NIRUFLOW scheduled rules\n[Service]\nType=oneshot\nExecStart=" + str(Path(exe).resolve()) + " run-enabled\nNoNewPrivileges=true\n")
    timer.write_text("[Unit]\nDescription=NIRUFLOW hourly scheduler\n[Timer]\nOnCalendar=hourly\nPersistent=true\nRandomizedDelaySec=3min\n[Install]\nWantedBy=timers.target\n")
    for args in (("daemon-reload",),("enable","--now",UNIT)):
        result=_call(*args)
        if result.returncode: raise RuntimeError(result.stderr.strip() or result.stdout.strip())
    return status()

def set_enabled(enabled):
    if not status()["installed"]:
        raise RuntimeError("Install scheduler first")
    result=_call("enable" if enabled else "disable", "--now", UNIT)
    if result.returncode: raise RuntimeError(result.stderr.strip() or result.stdout.strip())
    return status()
