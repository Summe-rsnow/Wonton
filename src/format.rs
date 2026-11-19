//! Pure formatting: timestamps, quota percentages and durations.
//! No NSH dependency, so `cargo test` covers it without an engine.

/// Milliseconds in a day.
const DAY_MS: i64 = 86_400_000;

/// Days from 1970-01-01 to `y-m-d` (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Gregorian `(year, month, day)` for a day count since 1970-01-01 (Howard
/// Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// UTC calendar date (`YYYY-MM-DD`), matching `Date.toISOString().slice(0, 10)`.
pub fn utc_date_ymd(now_ms: i64) -> String {
    let days = now_ms.div_euclid(DAY_MS);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Default export filename aligned with Antigravity-Manager.
pub fn default_export_filename(now_ms: i64) -> String {
    format!("antigravity_accounts_{}.json", utc_date_ymd(now_ms))
}

/// Formats Unix seconds as `YYYY-MM-DDTHH:MM:SS.000000Z` for credential stores.
pub fn rfc3339_utc_secs(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86_400);
    let secs_of_day = unix_secs.rem_euclid(86_400);
    let hour = secs_of_day / 3_600;
    let minute = (secs_of_day % 3_600) / 60;
    let second = secs_of_day % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.000000Z")
}

/// Parses an RFC 3339 timestamp (`2026-11-14T12:34:56.789Z`, or with a
/// `+hh:mm` offset) to Unix milliseconds. Returns `None` for anything else,
/// so a malformed `resetTime` shows no countdown instead of a wrong one.
pub fn parse_rfc3339(text: &str) -> Option<i64> {
    let text = text.trim();
    let (date, rest) = text.split_once('T')?;
    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let (clock, offset_minutes) = match rest.strip_suffix('Z') {
        Some(clock) => (clock, 0),
        None => {
            let (clock, offset) = match rest.rfind(['+', '-']) {
                Some(at) => {
                    let sign = if rest.as_bytes()[at] == b'-' { -1 } else { 1 };
                    let offset = &rest[at + 1..];
                    let (hours, minutes) = offset.split_once(':')?;
                    let hours: i64 = hours.parse().ok()?;
                    let minutes: i64 = minutes.parse().ok()?;
                    (&rest[..at], sign * (hours * 60 + minutes))
                }
                None => (rest, 0),
            };
            (clock, offset)
        }
    };

    let mut parts = clock.split(':');
    let hour: i64 = parts.next()?.parse().ok()?;
    let minute: i64 = parts.next()?.parse().ok()?;
    let seconds = parts.next().unwrap_or("0");
    let (second, fraction_ms) = match seconds.split_once('.') {
        Some((second, fraction)) => {
            let mut fraction = fraction.to_owned();
            fraction.truncate(3);
            while fraction.len() < 3 {
                fraction.push('0');
            }
            (second.parse::<i64>().ok()?, fraction.parse::<i64>().unwrap_or(0))
        }
        None => (seconds.parse::<i64>().ok()?, 0),
    };
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    let days = days_from_civil(year, month, day);
    let millis = days * DAY_MS
        + hour * 3_600_000
        + minute * 60_000
        + second * 1_000
        + fraction_ms
        - offset_minutes * 60_000;
    Some(millis)
}

/// `78` for `0.78`.
pub fn percent(fraction: f32) -> u32 {
    (fraction.clamp(0.0, 1.0) * 100.0).round() as u32
}

/// A duration in the shortest compact unit: `4小时38分`, `6天23小时`.
fn duration_label(ms: i64) -> String {
    let minutes = ms.max(0) / 60_000;
    if minutes < 1 {
        return "<1分钟".to_owned();
    }
    if minutes < 60 {
        return format!("{minutes}分钟");
    }
    let hours = minutes / 60;
    if hours < 24 {
        let rest = minutes % 60;
        return if rest == 0 {
            format!("{hours}小时")
        } else {
            format!("{hours}小时{rest}分")
        };
    }
    let days = hours / 24;
    let rest = hours % 24;
    if rest == 0 {
        format!("{days}天")
    } else {
        format!("{days}天{rest}小时")
    }
}

/// Concise countdown until quota recovery (`4小时38分`, `6天23小时`, `即将`),
/// or empty when upstream gave no readable time.
pub fn reset_label(reset: Option<&str>, now_ms: i64) -> String {
    let Some(reset) = reset else {
        return String::new();
    };
    let Some(at) = parse_rfc3339(reset) else {
        return String::new();
    };
    if at <= now_ms {
        return "即将".to_owned();
    }
    duration_label(at - now_ms)
}

/// When the quota was last read: `21分钟前`, `刚刚`, `未刷新`.
pub fn age_label(fetched_at: Option<i64>, now_ms: i64) -> String {
    match fetched_at {
        None => "未刷新".to_owned(),
        Some(at) if now_ms - at < 60_000 => "刚刚".to_owned(),
        Some(at) => format!("{}前", duration_label(now_ms - at)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-11-14T12:34:56Z.
    const NOON: i64 = 1_794_659_696_000;

    #[test]
    fn parses_utc_and_offsets() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339("2026-11-14T12:34:56Z"), Some(NOON));
        assert_eq!(parse_rfc3339("2026-11-14T12:34:56.789Z"), Some(NOON + 789));
        assert_eq!(parse_rfc3339("2026-11-14T20:34:56+08:00"), Some(NOON));
    }

    #[test]
    fn rejects_what_it_cannot_read() {
        assert_eq!(parse_rfc3339(""), None);
        assert_eq!(parse_rfc3339("2026-11-14"), None);
        assert_eq!(parse_rfc3339("not a time"), None);
    }

    #[test]
    fn labels_a_reset_time() {
        assert_eq!(reset_label(Some("2026-11-14T14:35:00Z"), NOON), "2小时");
        assert_eq!(reset_label(Some("2026-11-14T17:12:56Z"), NOON), "4小时38分");
        assert_eq!(reset_label(Some("2026-11-21T11:34:56Z"), NOON), "6天23小时");
        assert_eq!(reset_label(Some("2026-11-14T12:00:00Z"), NOON), "即将");
        assert_eq!(reset_label(None, NOON), "");
        assert_eq!(reset_label(Some("bogus"), NOON), "");
    }

    #[test]
    fn labels_an_age() {
        assert_eq!(age_label(None, NOON), "未刷新");
        assert_eq!(age_label(Some(NOON - 5_000), NOON), "刚刚");
        assert_eq!(age_label(Some(NOON - 3 * 60_000), NOON), "3分钟前");
        assert_eq!(age_label(Some(NOON - 26 * 3_600_000), NOON), "1天2小时前");
    }

    #[test]
    fn rounds_a_fraction_to_a_percentage() {
        assert_eq!(percent(0.784), 78);
        assert_eq!(percent(0.0), 0);
        assert_eq!(percent(1.0), 100);
        assert_eq!(percent(1.4), 100);
    }

    #[test]
    fn formats_utc_export_filename() {
        assert_eq!(utc_date_ymd(0), "1970-01-01");
        assert_eq!(utc_date_ymd(-1), "1969-12-31");
        assert_eq!(utc_date_ymd(NOON), "2026-11-14");
        assert_eq!(default_export_filename(NOON), "antigravity_accounts_2026-11-14.json");
        assert_eq!(rfc3339_utc_secs(NOON / 1000), "2026-11-14T12:34:56.000000Z");
    }
}
