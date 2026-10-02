//! Hourly averaging, QC, correction, NowCast and daily summaries.

use crate::aqi::{self, Category};
use crate::correction::{channels_agree, epa_extended};
use crate::input::Record;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Options {
    /// Fixed offset from UTC in seconds used for local hours and days.
    pub utc_offset: i64,
    /// Minimum fraction of expected samples in an hour (EPA: 0.9).
    pub min_completeness: f64,
    /// Minimum valid hours for a daily mean to count (default 18 = 75 %).
    pub min_day_hours: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            utc_offset: 0,
            min_completeness: 0.9,
            min_day_hours: 18,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    Valid,
    /// Fewer complete samples than the completeness threshold.
    Incomplete,
    /// Channels A and B disagree (> 5 µg/m³ and > 70 %).
    ChannelDisagree,
}

impl Flag {
    pub fn name(self) -> &'static str {
        match self {
            Flag::Valid => "valid",
            Flag::Incomplete => "incomplete",
            Flag::ChannelDisagree => "ab_disagree",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Hour {
    /// Local hour number: (utc_seconds + offset) / 3600.
    pub key: i64,
    pub samples: usize,
    pub expected: usize,
    pub a: Option<f64>,
    pub b: Option<f64>,
    pub rh: Option<f64>,
    pub flag: Flag,
    /// Corrected PM2.5 (only for valid hours).
    pub corrected: Option<f64>,
    pub nowcast: Option<f64>,
}

impl Hour {
    pub fn raw(&self) -> Option<f64> {
        Some((self.a? + self.b?) / 2.0)
    }
}

#[derive(Debug, Clone)]
pub struct Day {
    /// Local day number (days since 1970-01-01 in local time).
    pub day: i64,
    pub valid_hours: usize,
    pub complete: bool,
    pub mean: Option<f64>,
    pub max_hour: Option<f64>,
}

impl Day {
    /// AQI of the daily mean, only when the day is complete.
    pub fn aqi(&self) -> Option<(u32, Category)> {
        if self.complete {
            aqi::pm25_aqi(self.mean?)
        } else {
            None
        }
    }
}

/// Median spacing between consecutive readings, in seconds.
pub fn sample_interval(records: &[Record]) -> i64 {
    let mut diffs: Vec<i64> = records
        .windows(2)
        .map(|w| w[1].ts - w[0].ts)
        .filter(|d| *d > 0)
        .collect();
    if diffs.is_empty() {
        return 120;
    }
    diffs.sort_unstable();
    diffs[diffs.len() / 2]
}

/// Number of readings expected per hour for a given sampling interval.
pub fn expected_per_hour(interval: i64) -> usize {
    if interval >= 3600 {
        1
    } else {
        ((3600.0 / interval as f64).round() as usize).max(1)
    }
}

#[derive(Default)]
struct Acc {
    n: usize,
    a: f64,
    b: f64,
    rh: f64,
}

/// Turn sorted, de-duplicated records into QC'd, corrected hours.
pub fn hourly(records: &[Record], opts: &Options) -> Vec<Hour> {
    let expected = expected_per_hour(sample_interval(records));
    let mut buckets: BTreeMap<i64, Acc> = BTreeMap::new();
    for r in records {
        let key = (r.ts + opts.utc_offset).div_euclid(3600);
        let acc = buckets.entry(key).or_default();
        if let (Some(a), Some(b), Some(rh)) = (r.a, r.b, r.rh) {
            acc.n += 1;
            acc.a += a;
            acc.b += b;
            acc.rh += rh;
        }
    }

    let mut hours: Vec<Hour> = buckets
        .into_iter()
        .map(|(key, acc)| {
            let (a, b, rh) = if acc.n > 0 {
                let n = acc.n as f64;
                (Some(acc.a / n), Some(acc.b / n), Some(acc.rh / n))
            } else {
                (None, None, None)
            };
            let complete = acc.n as f64 >= opts.min_completeness * expected as f64 - 1e-9;
            let flag = match (a, b) {
                _ if !complete || acc.n == 0 => Flag::Incomplete,
                (Some(a), Some(b)) if !channels_agree(a, b) => Flag::ChannelDisagree,
                _ => Flag::Valid,
            };
            let corrected = match (flag, a, b, rh) {
                (Flag::Valid, Some(a), Some(b), Some(rh)) => Some(epa_extended((a + b) / 2.0, rh)),
                _ => None,
            };
            Hour {
                key,
                samples: acc.n,
                expected,
                a,
                b,
                rh,
                flag,
                corrected,
                nowcast: None,
            }
        })
        .collect();

    let valid: BTreeMap<i64, f64> = hours
        .iter()
        .filter_map(|h| Some((h.key, h.corrected?)))
        .collect();
    for h in &mut hours {
        let recent: Vec<Option<f64>> = (0..12).map(|i| valid.get(&(h.key - i)).copied()).collect();
        h.nowcast = aqi::nowcast(&recent);
    }
    hours
}

/// Group valid hours into local days.
pub fn daily(hours: &[Hour], opts: &Options) -> Vec<Day> {
    let mut by_day: BTreeMap<i64, Vec<f64>> = BTreeMap::new();
    for h in hours {
        let entry = by_day.entry(h.key.div_euclid(24)).or_default();
        if let Some(c) = h.corrected {
            entry.push(c);
        }
    }
    by_day
        .into_iter()
        .map(|(day, vals)| {
            let n = vals.len();
            let mean = (n > 0).then(|| vals.iter().sum::<f64>() / n as f64);
            let max_hour = vals.iter().cloned().reduce(f64::max);
            Day {
                day,
                valid_hours: n,
                complete: n >= opts.min_day_hours,
                mean,
                max_hour,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(ts: i64, a: f64, b: f64, rh: f64) -> Record {
        Record {
            ts,
            a: Some(a),
            b: Some(b),
            rh: Some(rh),
        }
    }

    /// 2-minute readings for `hours` hours starting at `start`.
    fn series(start: i64, hours: i64, a: f64, b: f64, rh: f64) -> Vec<Record> {
        (0..hours * 30)
            .map(|i| rec(start + i * 120, a, b, rh))
            .collect()
    }

    #[test]
    fn interval_and_expected() {
        let s = series(0, 2, 1.0, 1.0, 50.0);
        assert_eq!(sample_interval(&s), 120);
        assert_eq!(expected_per_hour(120), 30);
        assert_eq!(expected_per_hour(3600), 1);
        assert_eq!(expected_per_hour(7200), 1);
    }

    #[test]
    fn valid_hour_is_corrected() {
        let s = series(0, 1, 10.0, 12.0, 50.0);
        let h = hourly(&s, &Options::default());
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].flag, Flag::Valid);
        assert_eq!(h[0].samples, 30);
        let want = epa_extended(11.0, 50.0);
        assert!((h[0].corrected.unwrap() - want).abs() < 1e-9);
    }

    #[test]
    fn incomplete_hour_is_flagged() {
        let mut s = series(0, 2, 10.0, 10.0, 50.0);
        // Remove 4 readings from the second hour → 26/30 = 87 % < 90 %.
        s.drain(30..34);
        let h = hourly(&s, &Options::default());
        assert_eq!(h[0].flag, Flag::Valid);
        assert_eq!(h[1].flag, Flag::Incomplete);
        assert_eq!(h[1].corrected, None);
    }

    #[test]
    fn missing_humidity_counts_as_missing_sample() {
        let mut s = series(0, 1, 10.0, 10.0, 50.0);
        for r in s.iter_mut().take(5) {
            r.rh = None;
        }
        let h = hourly(&s, &Options::default());
        assert_eq!(h[0].samples, 25);
        assert_eq!(h[0].flag, Flag::Incomplete);
    }

    #[test]
    fn disagreeing_channels_flagged() {
        let s = series(0, 1, 2.0, 30.0, 50.0);
        let h = hourly(&s, &Options::default());
        assert_eq!(h[0].flag, Flag::ChannelDisagree);
        assert_eq!(h[0].corrected, None);
    }

    #[test]
    fn hourly_input_is_one_sample_per_hour() {
        let s: Vec<Record> = (0..5).map(|i| rec(i * 3600, 10.0, 10.0, 50.0)).collect();
        let h = hourly(&s, &Options::default());
        assert_eq!(h.len(), 5);
        assert!(h.iter().all(|h| h.flag == Flag::Valid && h.expected == 1));
    }

    #[test]
    fn offset_shifts_hours_and_days() {
        // Readings 06:00–07:59 UTC; at UTC-7 that is 23:00–00:59 local,
        // spanning two local days.
        let s = series(6 * 3600, 2, 10.0, 10.0, 50.0);
        let opts = Options {
            utc_offset: -7 * 3600,
            min_day_hours: 1,
            ..Options::default()
        };
        let h = hourly(&s, &opts);
        assert_eq!(h[0].key, -1);
        let d = daily(&h, &opts);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].day, -1);
        assert_eq!(d[1].day, 0);
    }

    #[test]
    fn daily_mean_and_completeness() {
        let mut s = series(0, 18, 10.0, 10.0, 50.0);
        s.extend(series(18 * 3600, 6, 30.0, 30.0, 50.0));
        s.extend(series(86_400, 5, 10.0, 10.0, 50.0));
        let opts = Options::default();
        let h = hourly(&s, &opts);
        let d = daily(&h, &opts);
        assert_eq!(d.len(), 2);
        let lo = epa_extended(10.0, 50.0);
        let hi = epa_extended(30.0, 50.0);
        assert_eq!(d[0].valid_hours, 24);
        assert!(d[0].complete);
        assert!((d[0].mean.unwrap() - (18.0 * lo + 6.0 * hi) / 24.0).abs() < 1e-9);
        assert!((d[0].max_hour.unwrap() - hi).abs() < 1e-9);
        assert!(d[0].aqi().is_some());
        assert!(!d[1].complete);
        assert_eq!(d[1].aqi(), None);
    }

    #[test]
    fn nowcast_filled_for_consecutive_valid_hours() {
        let s = series(0, 3, 10.0, 10.0, 50.0);
        let h = hourly(&s, &Options::default());
        assert_eq!(h[0].nowcast, None);
        let c = epa_extended(10.0, 50.0);
        assert!((h[1].nowcast.unwrap() - c).abs() < 1e-9);
        assert!((h[2].nowcast.unwrap() - c).abs() < 1e-9);
    }
}
