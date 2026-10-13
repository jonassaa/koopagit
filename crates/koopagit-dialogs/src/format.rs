//! Formatting helpers shared by dialogs.

use std::time::{SystemTime, UNIX_EPOCH};

/// Current local wall-clock time as `HH:MM:SS` (UTC; no time-zone database).
pub fn now_hms() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let s = secs.rem_euclid(86_400);
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// Formats a commit time like TortoiseGit's short date/time format
/// (`LogDateFormat`, default on): `YYYY-MM-DD HH:MM`, in the commit's own offset.
pub fn short_datetime(time: i64, offset: i32) -> String {
    let local = time + i64::from(offset);
    let days = local.div_euclid(86_400);
    let secs = local.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        secs / 3600,
        (secs / 60) % 60
    )
}

/// `YYYY-MM-DDTHH:MM:SS` in the given offset (the author-date picker format).
pub fn iso_datetime(time: i64, offset: i32) -> String {
    let local = time + i64::from(offset);
    let (y, m, d) = civil_from_days(local.div_euclid(86_400));
    let secs = local.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}",
        secs / 3600,
        (secs / 60) % 60,
        secs % 60
    )
}

/// Seconds since the epoch.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Relative time ("Relative Times in log" setting), e.g. `3 days ago`.
pub fn relative_time(time: i64, now: i64) -> String {
    let d = (now - time).max(0);
    let (n, unit) = match d {
        0..60 => (d, "second"),
        60..3_600 => (d / 60, "minute"),
        3_600..86_400 => (d / 3_600, "hour"),
        86_400..604_800 => (d / 86_400, "day"),
        604_800..2_629_800 => (d / 604_800, "week"),
        2_629_800..31_557_600 => (d / 2_629_800, "month"),
        _ => (d / 31_557_600, "year"),
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

// Howard Hinnant's days-to-civil algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(short_datetime(0, 0), "1970-01-01 00:00");
        assert_eq!(short_datetime(1_700_000_000, 3600), "2023-11-14 23:13");
        assert_eq!(relative_time(0, 90), "1 minute ago");
        assert_eq!(iso_datetime(1_700_000_000, 0), "2023-11-14T22:13:20");
        assert_eq!(relative_time(0, 3 * 86_400), "3 days ago");
    }
}
