//! Display formatting (sizes, regional dates) and the ISO-8601 timestamps
//! Python's `datetime.isoformat()` writes into history and JSON output.
use std::time::{SystemTime, UNIX_EPOCH};

/// `human_size`: decimal units, one decimal with thousands separators.
pub fn human_size(value: f64) -> String {
    let labels = ["B", "kB", "MB", "GB", "TB"];
    let mut v = value;
    let mut i = 0;
    while v.abs() >= 1000.0 && i < labels.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    if i == 0 {
        return format!("{} B", v as i64);
    }
    format!("{} {}", thousands(&format!("{v:.1}")), labels[i])
}

fn thousands(s: &str) -> String {
    let (neg, s) = s.strip_prefix('-').map_or((false, s), |r| (true, r));
    let (int, frac) = s.split_once('.').unwrap_or((s, ""));
    let mut out = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if !frac.is_empty() {
        out.push('.');
        out.push_str(frac);
    }
    if neg {
        format!("-{out}")
    } else {
        out
    }
}

pub fn now_f64() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Days since 1970-01-01 for a civil date (proleptic Gregorian).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Broken-down time (year, month, day, hour, minute, second, micro, weekday
/// with Monday = 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Civil {
    pub y: i64,
    pub mo: i64,
    pub d: i64,
    pub h: i64,
    pub mi: i64,
    pub s: i64,
    pub us: i64,
}

impl Civil {
    pub fn weekday(&self) -> usize {
        // 1970-01-01 was a Thursday (3 with Monday = 0).
        (days_from_civil(self.y, self.mo, self.d) + 3).rem_euclid(7) as usize
    }
}

pub fn utc_civil(t: f64) -> Civil {
    let total_us = (t * 1e6).round() as i64;
    let secs = total_us.div_euclid(1_000_000);
    let us = total_us.rem_euclid(1_000_000);
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (y, mo, d) = civil_from_days(days);
    Civil {
        y,
        mo,
        d,
        h: rem / 3600,
        mi: rem % 3600 / 60,
        s: rem % 60,
        us,
    }
}

/// Local time for a POSIX timestamp (the process time zone).
pub fn local_civil(t: f64) -> Civil {
    let secs = t.floor() as libc::time_t;
    let us = ((t - t.floor()) * 1e6).round() as i64;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    Civil {
        y: tm.tm_year as i64 + 1900,
        mo: tm.tm_mon as i64 + 1,
        d: tm.tm_mday as i64,
        h: tm.tm_hour as i64,
        mi: tm.tm_min as i64,
        s: tm.tm_sec as i64,
        us: us.min(999_999),
    }
}

/// `datetime.isoformat()` of an aware UTC datetime: microseconds only when
/// non-zero, then `+00:00`.
pub fn iso_utc(t: f64) -> String {
    let c = utc_civil(t);
    iso(&c, "+00:00")
}

pub fn iso(c: &Civil, offset: &str) -> String {
    let mut s = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        c.y, c.mo, c.d, c.h, c.mi, c.s
    );
    if c.us != 0 {
        s.push_str(&format!(".{:06}", c.us));
    }
    s.push_str(offset);
    s
}

/// A parsed ISO-8601 timestamp: the civil fields as written and the UTC
/// offset in seconds when one was given.
#[derive(Clone, Debug, PartialEq)]
pub struct Iso {
    pub c: Civil,
    pub offset: Option<i64>,
}

impl Iso {
    /// POSIX seconds; a naive time is taken as UTC.
    pub fn epoch(&self) -> f64 {
        let days = days_from_civil(self.c.y, self.c.mo, self.c.d);
        let secs =
            days * 86400 + self.c.h * 3600 + self.c.mi * 60 + self.c.s - self.offset.unwrap_or(0);
        secs as f64 + self.c.us as f64 / 1e6
    }
    pub fn offset_text(&self) -> String {
        match self.offset {
            None => String::new(),
            Some(o) => {
                let sign = if o < 0 { '-' } else { '+' };
                let a = o.abs();
                format!("{sign}{:02}:{:02}", a / 3600, a % 3600 / 60)
            }
        }
    }
    pub fn isoformat(&self) -> String {
        iso(&self.c, &self.offset_text())
    }
}

/// Python 3.11 `datetime.fromisoformat` for the forms NIRUFLOW meets:
/// `YYYY-MM-DD[THH:MM[:SS[.fff…]]][Z|±HH:MM]` (fractions beyond 6 digits
/// are truncated).
pub fn parse_iso(s: &str) -> Option<Iso> {
    let s = s.trim();
    let b = s.as_bytes();
    let num = |a: usize, n: usize| -> Option<i64> {
        let part = s.get(a..a + n)?;
        part.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| part.parse().ok())?
    };
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let (y, mo, d) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    let mut c = Civil {
        y,
        mo,
        d,
        h: 0,
        mi: 0,
        s: 0,
        us: 0,
    };
    let mut i = 10;
    let mut offset = None;
    if i < b.len() {
        if b[i] != b'T' && b[i] != b' ' {
            return None;
        }
        i += 1;
        c.h = num(i, 2)?;
        i += 2;
        if b.get(i) != Some(&b':') {
            return None;
        }
        c.mi = num(i + 1, 2)?;
        i += 3;
        if b.get(i) == Some(&b':') {
            c.s = num(i + 1, 2)?;
            i += 3;
            if matches!(b.get(i), Some(b'.') | Some(b',')) {
                i += 1;
                let start = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                if i == start {
                    return None;
                }
                let mut frac = s[start..i.min(start + 6)].to_string();
                while frac.len() < 6 {
                    frac.push('0');
                }
                c.us = frac.parse().ok()?;
            }
        }
        if c.h > 23 || c.mi > 59 || c.s > 59 {
            return None;
        }
        if i < b.len() {
            match b[i] {
                b'Z' if i + 1 == b.len() => offset = Some(0),
                b'+' | b'-' => {
                    let sign = if b[i] == b'-' { -1 } else { 1 };
                    let oh = num(i + 1, 2)?;
                    let om = if b.get(i + 3) == Some(&b':') {
                        num(i + 4, 2)?
                    } else {
                        num(i + 3, 2)?
                    };
                    offset = Some(sign * (oh * 3600 + om * 60));
                }
                _ => return None,
            }
        }
    }
    Some(Iso { c, offset })
}

pub const REGIONS: [&str; 3] = ["sv_SE", "en_GB", "en_US"];
pub const DATE_PATTERNS: [&str; 3] = ["yyyy-MM-dd", "dd/MM/yyyy", "MM/dd/yyyy"];
pub const TIME_PATTERNS: [&str; 2] = ["HH:mm", "hh:mm AP"];

fn weekday_name(region: &str, wd: usize) -> &'static str {
    const SV: [&str; 7] = ["mån", "tis", "ons", "tors", "fre", "lör", "sön"];
    const EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    match region {
        "en_GB" | "en_US" => EN[wd % 7],
        _ => SV[wd % 7],
    }
}

/// The user's regional date/time display.
#[derive(Clone, Debug)]
pub struct DateStyle {
    pub region: String,
    pub date: String,
    pub time: String,
    pub weekday: bool,
}

impl DateStyle {
    pub fn civil(&self, c: &Civil) -> String {
        let date = match self.date.as_str() {
            "dd/MM/yyyy" => format!("{:02}/{:02}/{:04}", c.d, c.mo, c.y),
            "MM/dd/yyyy" => format!("{:02}/{:02}/{:04}", c.mo, c.d, c.y),
            _ => format!("{:04}-{:02}-{:02}", c.y, c.mo, c.d),
        };
        let time = match self.time.as_str() {
            "hh:mm AP" => {
                let h12 = if c.h % 12 == 0 { 12 } else { c.h % 12 };
                format!(
                    "{:02}:{:02} {}",
                    h12,
                    c.mi,
                    if c.h < 12 { "AM" } else { "PM" }
                )
            }
            _ => format!("{:02}:{:02}", c.h, c.mi),
        };
        let mut s = format!("{date} {time}");
        if self.weekday {
            s = format!("{} {s}", weekday_name(&self.region, c.weekday()));
        }
        s
    }

    /// A POSIX timestamp in local time.
    pub fn timestamp(&self, t: f64) -> String {
        self.civil(&local_civil(t))
    }

    /// An ISO string: an aware time is shown in local time, a naive one as
    /// written, anything else unchanged.
    pub fn iso(&self, s: &str) -> String {
        match parse_iso(s) {
            Some(i) if i.offset.is_some() => self.timestamp(i.epoch()),
            Some(i) => self.civil(&i.c),
            None => s.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(r: &str, d: &str, t: &str, w: bool) -> DateStyle {
        DateStyle {
            region: r.into(),
            date: d.into(),
            time: t.into(),
            weekday: w,
        }
    }

    #[test]
    fn sizes_match_the_python_version() {
        assert_eq!(human_size(1063435190.0), "1.1 GB");
        assert_eq!(human_size(541249161.0), "541.2 MB");
        assert_eq!(human_size(0.0), "0 B");
        assert_eq!(human_size(999.0), "999 B");
        assert_eq!(human_size(1.5e18), "1,500,000.0 TB");
    }

    #[test]
    fn regional_formats() {
        let c = Civil {
            y: 2026,
            mo: 10,
            d: 9,
            h: 22,
            mi: 0,
            s: 0,
            us: 0,
        };
        assert_eq!(
            style("sv_SE", "yyyy-MM-dd", "HH:mm", true).civil(&c),
            "fre 2026-10-09 22:00"
        );
        assert_eq!(
            style("en_GB", "dd/MM/yyyy", "HH:mm", true).civil(&c),
            "Fri 09/10/2026 22:00"
        );
        assert_eq!(
            style("en_US", "MM/dd/yyyy", "hh:mm AP", true).civil(&c),
            "Fri 10/09/2026 10:00 PM"
        );
        assert_eq!(
            style("sv_SE", "unknown", "HH:mm", false).civil(&c),
            "2026-10-09 22:00"
        );
        assert_eq!(
            style("sv_SE", "yyyy-MM-dd", "HH:mm", true).iso("2026-10-09T22:00:00"),
            "fre 2026-10-09 22:00"
        );
    }

    #[test]
    fn iso_round_trip() {
        assert_eq!(iso_utc(1_600_000_000.0), "2020-09-13T12:26:40+00:00");
        assert_eq!(iso_utc(1_600_000_000.5), "2020-09-13T12:26:40.500000+00:00");
        let p = parse_iso("2024-01-02T03:04:05.123456789Z").unwrap();
        assert_eq!(p.isoformat(), "2024-01-02T03:04:05.123456+00:00");
        let p = parse_iso("2024-01-02T03:04:05+02:00").unwrap();
        assert_eq!(p.isoformat(), "2024-01-02T03:04:05+02:00");
        assert_eq!(
            p.epoch(),
            parse_iso("2024-01-02T01:04:05Z").unwrap().epoch()
        );
        assert!(parse_iso("garbage").is_none());
        assert_eq!(
            civil_from_days(days_from_civil(2026, 11, 22)),
            (2026, 11, 22)
        );
    }
}
