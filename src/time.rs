//! Minimal UTC timestamp parsing and formatting, no time-zone database.

/// Days since 1970-01-01 for a proleptic Gregorian date.
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_civil`].
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Parse a PurpleAir timestamp into Unix seconds (UTC).
///
/// Accepts SD-card style `2024/07/01T00:02:13z`, ISO 8601
/// `2024-07-01T00:02:13Z` / `2024-07-01 00:02:13` (optionally with a
/// trailing `+00:00` or ` UTC`), and Unix seconds or milliseconds.
pub fn parse_timestamp(s: &str) -> Option<i64> {
    let s = s.trim().trim_matches('"');
    if s.is_empty() {
        return None;
    }
    if s.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        let v: f64 = s.parse().ok()?;
        let secs = if v > 1e11 { v / 1000.0 } else { v };
        return Some(secs.floor() as i64);
    }
    let mut s = s.to_string();
    for suffix in [" UTC", "UTC", "+00:00", "+0000", "Z", "z"] {
        if let Some(stripped) = s.strip_suffix(suffix) {
            s = stripped.trim_end().to_string();
            break;
        }
    }
    let bytes = s.as_bytes();
    if bytes.len() < 16 {
        return None;
    }
    let num = |a: usize, b: usize| -> Option<i64> { s.get(a..b)?.parse().ok() };
    let y = num(0, 4)?;
    let mo = num(5, 7)?;
    let d = num(8, 10)?;
    let sep_ok = matches!(bytes[4], b'-' | b'/') && bytes[7] == bytes[4];
    if !sep_ok || !matches!(bytes[10], b'T' | b' ') || bytes[13] != b':' {
        return None;
    }
    let h = num(11, 13)?;
    let mi = num(14, 16)?;
    let (sec, mut rest) = if bytes.len() >= 19 && bytes[16] == b':' {
        (num(17, 19)?, s.get(19..)?)
    } else {
        (0, s.get(16..)?)
    };
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    // Fractional seconds are ignored.
    if let Some(frac) = rest.strip_prefix('.') {
        rest = frac.trim_start_matches(|c: char| c.is_ascii_digit());
    }
    // A trailing numeric UTC offset (e.g. `-07:00`, ` -0700`) is applied;
    // anything else after the time is rejected rather than silently ignored.
    let rest = rest.trim();
    let offset = if rest.is_empty() {
        0
    } else if matches!(rest.as_bytes()[0], b'+' | b'-') {
        parse_offset(rest)?
    } else {
        return None;
    };
    Some(days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + sec - offset)
}

/// Parse a fixed UTC offset like `-07:00`, `+0530`, `-7`, `UTC` into seconds.
pub fn parse_offset(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("utc") || s.eq_ignore_ascii_case("z") {
        return Some(0);
    }
    let (sign, rest) = match s.as_bytes().first()? {
        b'+' => (1, &s[1..]),
        b'-' => (-1, &s[1..]),
        _ => (1, s),
    };
    let (h, m) = if let Some((h, m)) = rest.split_once(':') {
        (h.parse::<i64>().ok()?, m.parse::<i64>().ok()?)
    } else if rest.len() == 4 {
        (rest[..2].parse().ok()?, rest[2..].parse().ok()?)
    } else {
        (rest.parse().ok()?, 0)
    };
    if h > 14 || m > 59 {
        return None;
    }
    Some(sign * (h * 3600 + m * 60))
}

/// Format local seconds (UTC seconds + offset) as `YYYY-MM-DD HH:MM`.
pub fn format_local_minute(local_secs: i64) -> String {
    let days = local_secs.div_euclid(86_400);
    let rem = local_secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        rem / 3600,
        (rem % 3600) / 60
    )
}

/// Format a local day number as `YYYY-MM-DD`.
pub fn format_day(day: i64) -> String {
    let (y, m, d) = civil_from_days(day);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_roundtrip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        for z in [-1000, 0, 19_000, 20_000, 30_000] {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z);
        }
    }

    #[test]
    fn parses_sd_card_format() {
        // 2024-07-01T00:02:13Z = 1719792133
        assert_eq!(parse_timestamp("2024/07/01T00:02:13z"), Some(1_719_792_133));
    }

    #[test]
    fn parses_iso_variants() {
        let t = Some(1_719_792_133);
        assert_eq!(parse_timestamp("2024-07-01T00:02:13Z"), t);
        assert_eq!(parse_timestamp("2024-07-01 00:02:13"), t);
        assert_eq!(parse_timestamp("2024-07-01T00:02:13+00:00"), t);
        assert_eq!(parse_timestamp("2024-07-01 00:02:13 UTC"), t);
        assert_eq!(parse_timestamp("\"2024-07-01T00:02:13Z\""), t);
    }

    #[test]
    fn parses_unix() {
        assert_eq!(parse_timestamp("1719792133"), Some(1_719_792_133));
        assert_eq!(parse_timestamp("1719792133000"), Some(1_719_792_133));
    }

    #[test]
    fn applies_non_utc_offsets() {
        // 2024-07-01T00:00:00-07:00 is 07:00 UTC.
        let t = Some(1_719_792_000 + 7 * 3600);
        assert_eq!(parse_timestamp("2024-07-01T00:00:00-07:00"), t);
        assert_eq!(parse_timestamp("2024-07-01 00:00:00 -0700"), t);
        assert_eq!(parse_timestamp("2024-07-01T09:00:00+02:00"), t);
        assert_eq!(parse_timestamp("2024-07-01T07:00:00.000Z"), t);
    }

    #[test]
    fn rejects_trailing_garbage() {
        assert_eq!(parse_timestamp("2024-07-01T00:00:00 PDT"), None);
        assert_eq!(parse_timestamp("2024-07-01T00:00:00xyz"), None);
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_timestamp(""), None);
        assert_eq!(parse_timestamp("yesterday"), None);
        assert_eq!(parse_timestamp("2024-13-01T00:00:00Z"), None);
        assert_eq!(parse_timestamp("2024/07-01T00:00:00Z"), None);
    }

    #[test]
    fn offsets() {
        assert_eq!(parse_offset("-07:00"), Some(-25_200));
        assert_eq!(parse_offset("+0530"), Some(19_800));
        assert_eq!(parse_offset("-7"), Some(-25_200));
        assert_eq!(parse_offset("UTC"), Some(0));
        assert_eq!(parse_offset("+99"), None);
        assert_eq!(parse_offset("abc"), None);
    }

    #[test]
    fn formatting() {
        assert_eq!(format_local_minute(1_719_792_133), "2024-07-01 00:02");
        assert_eq!(
            format_local_minute(1_719_792_133 - 25_200),
            "2024-06-30 17:02"
        );
        assert_eq!(format_day(days_from_civil(2024, 2, 29)), "2024-02-29");
    }
}
