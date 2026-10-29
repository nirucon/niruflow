"""Neutral monochrome palettes and NIRUX desktop theme following."""
from pathlib import Path
import os
import re

try:
    import tomllib
except ImportError:  # Python 3.10
    tomllib = None

THEMES={
 'Niru Noir':('#131415','#202123','#e5e5e5','#9a9da0','#35373a','#2a2c2f'),
 'Satie':('#191919','#252422','#eee9df','#b5afa6','#44423f','#33312e'),
 'C. Larsson':('#20211e','#2b2d29','#ece9dd','#b9bbaf','#494c45','#373a34'),
}
FOLLOW_NIRUX='NIRUX desktop'
# Fallbacks mirror the ten palettes NIRUWM compiles in. A user theme file in
# ~/.config/niruwm/themes takes precedence, so custom NIRUX themes also work.
NIRUX_BUILTINS={
 'niru-noir':('#0f0f10','#e5e5e5','#a8a8a8','#2a2a2d','#5a5a60'),
 'niru-nord':('#2e3440','#eceff4','#9aa5b1','#3b4252','#88c0d0'),
 'niru-satie':('#141414','#dedad4','#8f8b86','#2a2724','#b08d7d'),
 'niru-carl-larsson':('#f3ece0','#3b3228','#7d6f5f','#d8cdbb','#a63d2f'),
 'niru-forest-vigil':('#0b100c','#d8ded2','#96a292','#2b362c','#8aa87e'),
 'niru-blood-of-urdr':('#120808','#e8d6cf','#b09a92','#3a1d1b','#c96050'),
 'niru-root-and-stone':('#17100b','#eadfd2','#b5a08c','#3a2a1e','#c19768'),
 'niru-blue-horizon':('#0b1821','#dce6eb','#93a6b2','#243846','#86aec8'),
 'niru-the-norns':('#111110','#ece9e4','#a6a19b','#3b3936','#c9c2b9'),
 'niru-winter-nights':('#1f232b','#ead9c3','#b9aa9a','#4e5157','#d8c4aa'),
}

def _colour(value, fallback):
    text=str(value).strip().strip('"\'')
    if not re.fullmatch(r'#?[0-9a-fA-F]{6}',text):
        return fallback
    return '#'+text.lstrip('#').lower()

def _mix(a,b,amount):
    a=a.lstrip('#'); b=b.lstrip('#')
    out=[]
    for i in (0,2,4):
        x=int(a[i:i+2],16); y=int(b[i:i+2],16)
        out.append(round(x+(y-x)*amount))
    return '#%02x%02x%02x'%tuple(out)

def nirux_theme_id(config_dir=None):
    base=Path(config_dir or os.environ.get('XDG_CONFIG_HOME',Path.home()/'.config'))/'niruwm'
    config=base/'config.toml'
    try:
        text=config.read_text(encoding='utf-8')
    except OSError:
        return 'niru-noir'
    found=re.search(r'^\s*theme\s*=\s*["\']([^"\']+)["\']',text,re.MULTILINE)
    return found.group(1) if found else 'niru-noir'

def _read_theme_file(path):
    if tomllib is not None:
        try:
            return tomllib.loads(path.read_text(encoding='utf-8'))
        except (OSError,ValueError):
            return {}
    values={}
    try:
        for line in path.read_text(encoding='utf-8').splitlines():
            match=re.match(r'\s*([A-Za-z_]+)\s*=\s*["\']([^"\']+)["\']',line)
            if match:
                values[match.group(1)]=match.group(2)
    except OSError:
        pass
    return values

def nirux_palette(config_dir=None,theme_id=None):
    theme_id=theme_id or nirux_theme_id(config_dir)
    base=Path(config_dir or os.environ.get('XDG_CONFIG_HOME',Path.home()/'.config'))/'niruwm'
    values=_read_theme_file(base/'themes'/f'{theme_id}.toml')
    fallback=NIRUX_BUILTINS.get(theme_id,NIRUX_BUILTINS['niru-noir'])
    bg=_colour(values.get('background',''),fallback[0])
    fg=_colour(values.get('foreground',''),fallback[1])
    muted=_colour(values.get('muted',''),fallback[2])
    border=_colour(values.get('border',''),fallback[3])
    focus=_colour(values.get('focus',''),fallback[4])
    panel=_mix(bg,fg,0.07)
    hover=_mix(bg,border,0.65)
    return bg,panel,fg,muted,border,hover,focus

def nirux_theme_signature(config_dir=None):
    """Stable token for the active NIRUX theme and its user override."""
    base=Path(config_dir or os.environ.get('XDG_CONFIG_HOME',Path.home()/'.config'))/'niruwm'
    theme_id=nirux_theme_id(config_dir)
    def mtime(path):
        try:
            return path.stat().st_mtime_ns
        except OSError:
            return 0
    return (theme_id, mtime(base/'config.toml'), mtime(base/'themes'/f'{theme_id}.toml'))

def theme_names():
    return [FOLLOW_NIRUX,*THEMES]

def stylesheet(theme):
    if theme==FOLLOW_NIRUX:
        bg,panel,fg,muted,border,hover,focus=nirux_palette()
        selection=focus
    else:
        bg,panel,fg,muted,border,hover=THEMES.get(theme,THEMES['Niru Noir'])
        selection=border
    return f'''
    QWidget {{background:{bg};color:{fg};font-family:"Noto Sans",sans-serif;font-size:12px;}}
    QLabel#brand {{font-size:13px;font-weight:650;letter-spacing:1px;}}
    QLabel#pageTitle {{font-size:20px;font-weight:600;padding-bottom:4px;}}
    QLabel#muted {{color:{muted};}}
    QFrame#sidebar {{background:{panel};border:1px solid {border};border-radius:6px;}}
    QPushButton {{background:{panel};border:1px solid {border};border-radius:5px;padding:8px 12px;text-align:center;}}
    QPushButton:hover {{background:{hover};}}
    QPushButton#nav {{text-align:left;background:transparent;border:0;padding:10px 12px;}}
    QPushButton#nav:checked {{background:{hover};border-left:2px solid {fg};font-weight:600;}}
    QPushButton#nav:hover {{background:{hover};}}
    QTableWidget,QPlainTextEdit {{background:{panel};alternate-background-color:{hover};border:1px solid {border};border-radius:5px;selection-background-color:{selection};}}
    QHeaderView::section {{background:{panel};color:{muted};border:0;border-bottom:1px solid {border};padding:9px;}}
    QTableWidget::item {{padding:4px;}}
    QComboBox,QSpinBox,QLineEdit {{background:{panel};border:1px solid {border};border-radius:5px;padding:7px;}}
    QComboBox:hover,QSpinBox:hover,QLineEdit:hover {{border-color:{muted};}}
    QStatusBar {{color:{muted};border-top:1px solid {border};}}
    QSplitter::handle {{background:{border};width:1px;}}
    QScrollBar:vertical {{background:{panel};width:10px;}}
    QScrollBar::handle:vertical {{background:{border};border-radius:4px;}}
    QCheckBox {{spacing:7px;}}
    '''
