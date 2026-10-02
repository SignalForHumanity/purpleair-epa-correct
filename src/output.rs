//! CSV writers and the plain-language summary.

use crate::aqi::{self, Category};
use crate::process::{Day, Flag, Hour};
use crate::time::{format_day, format_local_minute};
use std::fmt::Write as _;
use std::io::Write;

fn f1(v: Option<f64>) -> String {
    v.map(|v| format!("{v:.1}")).unwrap_or_default()
}

/// Truncated to 0.1 like the AQI calculation, so a displayed value never
/// disagrees with the AQI category next to it (35.46 shows as 35.4, not 35.5).
fn t1(v: Option<f64>) -> String {
    f1(v.map(aqi::truncate1))
}

pub fn write_hourly<W: Write>(w: W, hours: &[Hour]) -> Result<(), String> {
    let mut out = csv::Writer::from_writer(w);
    let err = |e: csv::Error| e.to_string();
    out.write_record([
        "local_hour_start",
        "samples",
        "expected_samples",
        "pm25_atm_a",
        "pm25_atm_b",
        "rh",
        "pm25_raw",
        "pm25_corrected",
        "flag",
        "nowcast",
        "nowcast_aqi",
        "nowcast_category",
    ])
    .map_err(err)?;
    for h in hours {
        let nc = h.nowcast.and_then(aqi::pm25_aqi);
        out.write_record([
            format_local_minute(h.key * 3600),
            h.samples.to_string(),
            h.expected.to_string(),
            f1(h.a),
            f1(h.b),
            f1(h.rh),
            f1(h.raw()),
            f1(h.corrected),
            h.flag.name().to_string(),
            t1(h.nowcast),
            nc.map(|(i, _)| i.to_string()).unwrap_or_default(),
            nc.map(|(_, c)| c.name().to_string()).unwrap_or_default(),
        ])
        .map_err(err)?;
    }
    out.flush().map_err(|e| e.to_string())
}

pub fn write_daily<W: Write>(w: W, days: &[Day]) -> Result<(), String> {
    let mut out = csv::Writer::from_writer(w);
    let err = |e: csv::Error| e.to_string();
    out.write_record([
        "local_date",
        "valid_hours",
        "complete",
        "pm25_corrected_mean",
        "pm25_max_hour",
        "aqi",
        "category",
    ])
    .map_err(err)?;
    for d in days {
        let a = d.aqi();
        out.write_record([
            format_day(d.day),
            d.valid_hours.to_string(),
            d.complete.to_string(),
            t1(d.mean),
            t1(d.max_hour),
            a.map(|(i, _)| i.to_string()).unwrap_or_default(),
            a.map(|(_, c)| c.name().to_string()).unwrap_or_default(),
        ])
        .map_err(err)?;
    }
    out.flush().map_err(|e| e.to_string())
}

/// Format an offset in seconds as `+HH:MM`.
pub fn format_offset(secs: i64) -> String {
    let sign = if secs < 0 { '-' } else { '+' };
    let s = secs.abs();
    format!("{sign}{:02}:{:02}", s / 3600, (s % 3600) / 60)
}

pub struct SummaryInput<'a> {
    pub files: usize,
    pub readings: usize,
    pub bad_rows: usize,
    pub interval: i64,
    pub utc_offset: i64,
    pub hours: &'a [Hour],
    pub days: &'a [Day],
}

pub fn summary(s: &SummaryInput) -> String {
    let mut o = String::new();
    let count = |f: Flag| s.hours.iter().filter(|h| h.flag == f).count();
    let _ = writeln!(
        o,
        "PurpleAir PM2.5, EPA-corrected (extended U.S. correction)"
    );
    let _ = writeln!(
        o,
        "Files: {}  Readings: {}  Unreadable rows: {}  Sampling interval: {} s",
        s.files, s.readings, s.bad_rows, s.interval
    );
    if let (Some(first), Some(last)) = (s.hours.first(), s.hours.last()) {
        let _ = writeln!(
            o,
            "Period (UTC{}): {} to {}",
            format_offset(s.utc_offset),
            format_local_minute(first.key * 3600),
            format_local_minute(last.key * 3600 + 3599)
        );
    }
    let _ = writeln!(
        o,
        "Hours: {} total, {} valid, {} incomplete, {} A/B channel disagreement",
        s.hours.len(),
        count(Flag::Valid),
        count(Flag::Incomplete),
        count(Flag::ChannelDisagree)
    );
    let complete: Vec<&Day> = s.days.iter().filter(|d| d.complete).collect();
    let _ = writeln!(
        o,
        "Days: {} with data, {} complete enough for a daily mean",
        s.days.len(),
        complete.len()
    );
    if complete.is_empty() {
        let _ = writeln!(o, "No complete days; no daily AQI can be reported.");
    } else {
        let _ = writeln!(o, "\nDaily AQI (24-hour mean, May 2024 breakpoints):");
        for cat in Category::ALL {
            let n = complete
                .iter()
                .filter(|d| d.aqi().map(|(_, c)| c) == Some(cat))
                .count();
            if n > 0 {
                let _ = writeln!(o, "  {:<32}{n:>5} day(s)", cat.name());
            }
        }
        let means: Vec<f64> = complete.iter().filter_map(|d| d.mean).collect();
        let avg = means.iter().sum::<f64>() / means.len() as f64;
        let worst = complete.iter().filter(|d| d.mean.is_some()).max_by(|a, b| {
            a.mean
                .partial_cmp(&b.mean)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let over = complete
            .iter()
            .filter(|d| d.mean.is_some_and(|m| aqi::truncate1(m) > 35.4))
            .count();
        let _ = writeln!(o, "\nMean of daily means: {avg:.1} µg/m³");
        if let Some(w) = worst {
            let _ = writeln!(
                o,
                "Highest day: {} at {} µg/m³",
                format_day(w.day),
                t1(w.mean)
            );
        }
        let _ = writeln!(
            o,
            "Days with a mean above 35.4 µg/m³ (AQI over 100): {over}"
        );
    }
    let _ = writeln!(
        o,
        "\nNote: corrected sensor data is useful for comparison but is not \
         regulatory data. The 24-hour (35 µg/m³) and annual (9.0 µg/m³) \
         standards are judged on multi-year statistics from reference monitors."
    );
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hour(key: i64, corrected: Option<f64>, flag: Flag) -> Hour {
        Hour {
            key,
            samples: 30,
            expected: 30,
            a: Some(10.0),
            b: Some(12.0),
            rh: Some(50.0),
            flag,
            corrected,
            nowcast: corrected,
        }
    }

    #[test]
    fn hourly_csv_shape() {
        let hours = vec![
            hour(0, Some(20.0), Flag::Valid),
            hour(1, None, Flag::Incomplete),
        ];
        let mut buf = Vec::new();
        write_hourly(&mut buf, &hours).unwrap();
        let s = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("local_hour_start,"));
        assert_eq!(
            lines[1],
            "1970-01-01 00:00,30,30,10.0,12.0,50.0,11.0,20.0,valid,20.0,71,Moderate"
        );
        assert!(lines[2].contains(",incomplete,,,"));
    }

    #[test]
    fn daily_csv_shape() {
        let days = vec![Day {
            day: 0,
            valid_hours: 20,
            complete: true,
            mean: Some(40.0),
            max_hour: Some(80.0),
        }];
        let mut buf = Vec::new();
        write_daily(&mut buf, &days).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("1970-01-01,20,true,40.0,80.0,112,Unhealthy for Sensitive Groups"));
    }

    #[test]
    fn displayed_daily_values_match_aqi_truncation() {
        // 35.46 truncates to 35.4 (Moderate, AQI 100) for the AQI; the CSV
        // must not show 35.5 next to a Moderate category.
        let days = vec![Day {
            day: 0,
            valid_hours: 24,
            complete: true,
            mean: Some(35.46),
            max_hour: Some(50.0),
        }];
        let mut buf = Vec::new();
        write_daily(&mut buf, &days).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(
            s.contains("1970-01-01,24,true,35.4,50.0,100,Moderate"),
            "{s}"
        );
        let text = summary(&SummaryInput {
            files: 1,
            readings: 720,
            bad_rows: 0,
            interval: 120,
            utc_offset: 0,
            hours: &[],
            days: &days,
        });
        assert!(text.contains("at 35.4 µg/m³"), "{text}");
        assert!(text.contains("AQI over 100): 0"));
    }

    #[test]
    fn summary_mentions_counts() {
        let hours = vec![
            hour(0, Some(40.0), Flag::Valid),
            hour(1, None, Flag::ChannelDisagree),
        ];
        let days = vec![Day {
            day: 0,
            valid_hours: 20,
            complete: true,
            mean: Some(40.0),
            max_hour: Some(40.0),
        }];
        let s = summary(&SummaryInput {
            files: 1,
            readings: 60,
            bad_rows: 0,
            interval: 120,
            utc_offset: -25_200,
            hours: &hours,
            days: &days,
        });
        assert!(s.contains("1 valid"));
        assert!(s.contains("1 A/B channel disagreement"));
        assert!(s.contains("Unhealthy for Sensitive Groups"));
        assert!(s.contains("AQI over 100): 1"));
        assert!(s.contains("UTC-07:00"));
    }

    #[test]
    fn offset_format() {
        assert_eq!(format_offset(19_800), "+05:30");
        assert_eq!(format_offset(-25_200), "-07:00");
    }
}
