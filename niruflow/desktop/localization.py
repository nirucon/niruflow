"""English interface with independently configurable regional date/time display."""
from datetime import datetime

REGIONS = ('sv_SE', 'en_GB', 'en_US')
DATE_PATTERNS = ('yyyy-MM-dd', 'dd/MM/yyyy', 'MM/dd/yyyy')
TIME_PATTERNS = ('HH:mm', 'hh:mm AP')
WEEKDAYS = {
    'sv_SE': ('mån', 'tis', 'ons', 'tors', 'fre', 'lör', 'sön'),
    'en_GB': ('Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'),
    'en_US': ('Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'),
}

def apply_locale(application, key):
    """Keep Qt widget language English; store regional choice separately."""
    from PySide6.QtCore import QLocale
    QLocale.setDefault(QLocale(QLocale.English, QLocale.UnitedKingdom))
    application.setProperty('niruflow_region', key if key in REGIONS else 'sv_SE')

def format_datetime(value, region='sv_SE', date_pattern='yyyy-MM-dd', time_pattern='HH:mm', weekday=False):
    """Format a local datetime without modifying the global process locale."""
    if isinstance(value, str):
        try:
            value = datetime.fromisoformat(value.replace('Z', '+00:00'))
            if value.tzinfo is not None:
                value = value.astimezone()
        except ValueError:
            return value
    if not isinstance(value, datetime):
        return str(value)
    date_pattern = date_pattern if date_pattern in DATE_PATTERNS else DATE_PATTERNS[0]
    time_pattern = time_pattern if time_pattern in TIME_PATTERNS else TIME_PATTERNS[0]
    date_formats = {'yyyy-MM-dd':'%Y-%m-%d', 'dd/MM/yyyy':'%d/%m/%Y', 'MM/dd/yyyy':'%m/%d/%Y'}
    time_formats = {'HH:mm':'%H:%M', 'hh:mm AP':'%I:%M %p'}
    text = value.strftime(date_formats[date_pattern] + ' ' + time_formats[time_pattern])
    if weekday:
        text = WEEKDAYS.get(region, WEEKDAYS['sv_SE'])[value.weekday()] + ' ' + text
    return text

def format_date(value, pattern='yyyy-MM-dd'):
    """Compatibility wrapper for older callers."""
    return format_datetime(value, date_pattern=pattern)

def tr(text):
    return text
