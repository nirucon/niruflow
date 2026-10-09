from datetime import datetime
from niruflow.desktop.localization import format_datetime

def test_swedish_default():
    assert format_datetime(datetime(2026,10,9,22,0),weekday=True) == 'fre 2026-10-09 22:00'

def test_english_uk():
    assert format_datetime(datetime(2026,10,9,22,0),'en_GB','dd/MM/yyyy','HH:mm',True) == 'Fri 09/10/2026 22:00'

def test_us_clock():
    assert format_datetime(datetime(2026,10,9,22,0),'en_US','MM/dd/yyyy','hh:mm AP',True) == 'Fri 10/09/2026 10:00 PM'

def test_iso_timestamp():
    assert format_datetime('2026-10-09T22:00:00',weekday=True) == 'fre 2026-10-09 22:00'

def test_invalid_pattern_fallback():
    assert format_datetime(datetime(2026,10,9,22,0),date_pattern='unknown') == '2026-10-09 22:00'
