"""Neutral monochrome palettes: no default coloured icons or buttons."""
THEMES={
 'Niru Noir':('#131415','#202123','#e5e5e5','#9a9da0','#35373a','#2a2c2f'),
 'Satie':('#191919','#252422','#eee9df','#b5afa6','#44423f','#33312e'),
 'C. Larsson':('#20211e','#2b2d29','#ece9dd','#b9bbaf','#494c45','#373a34'),
}
def stylesheet(theme):
    bg,panel,fg,muted,border,hover=THEMES.get(theme,THEMES['Niru Noir'])
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
    QTableWidget,QPlainTextEdit {{background:{panel};alternate-background-color:{hover};border:1px solid {border};border-radius:5px;selection-background-color:{border};}}
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
