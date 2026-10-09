"""Read-only environment diagnostics."""
from pathlib import Path
import shutil
import sys
from ..core.config import CONFIG, STATE

def check():
    try:
        import PySide6
        qt="available"
    except ImportError:
        qt="missing"
    return {"python":sys.version.split()[0],"pyside6":qt,"docker":bool(shutil.which("docker")),"systemctl":bool(shutil.which("systemctl")),"config":str(CONFIG),"config_exists":CONFIG.exists(),"state_dir":str(STATE),"quarantine_exists":(STATE/"quarantine").exists(),"message":"Read-only diagnostic"}
