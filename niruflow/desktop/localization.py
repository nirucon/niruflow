"""Regional preferences independent of the English UI."""
from datetime import datetime

LOCALES = {'sv_SE':'sv_SE','en_GB':'en_GB','en_US':'en_US'}

def apply_locale(application, key):
    from PySide6.QtCore import QLocale
    locale=QLocale(LOCALES.get(key,'sv_SE'))
    QLocale.setDefault(locale)
    application.setProperty('niruflow_locale',key)

def format_date(value, pattern='yyyy-MM-dd'):
    from PySide6.QtCore import QDateTime
    if isinstance(value,datetime):
        return QDateTime(value).toString(pattern+' HH:mm')
    return str(value)

def tr(text):
    return text
