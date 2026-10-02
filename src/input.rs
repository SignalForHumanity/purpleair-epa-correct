//! Reading PurpleAir CSV files (SD card and API / Data Download Tool formats).

use crate::time::parse_timestamp;
use std::io::Read;

/// One raw reading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Record {
    /// Unix seconds, UTC.
    pub ts: i64,
    /// Channel A PM2.5 ATM, µg/m³.
    pub a: Option<f64>,
    /// Channel B PM2.5 ATM, µg/m³.
    pub b: Option<f64>,
    /// Sensor relative humidity, %.
    pub rh: Option<f64>,
}

const TIME_COLS: &[&str] = &[
    "utcdatetime",
    "time_stamp",
    "timestamp",
    "datetime",
    "date_time",
    "created_at",
    "time",
];
const A_COLS: &[&str] = &["pm2_5_atm", "pm2.5_atm_a", "pm2_5_atm_a", "pm25_atm_a"];
const B_COLS: &[&str] = &["pm2_5_atm_b", "pm2.5_atm_b", "pm25_atm_b"];
const RH_COLS: &[&str] = &["current_humidity", "humidity", "humidity_a", "rh"];

fn find(headers: &[String], names: &[&str]) -> Option<usize> {
    names
        .iter()
        .find_map(|n| headers.iter().position(|h| h == n))
}

fn num(field: Option<&str>) -> Option<f64> {
    let v: f64 = field?.trim().trim_matches('"').parse().ok()?;
    v.is_finite().then_some(v)
}

/// Result of reading one file.
#[derive(Debug, Default)]
pub struct ReadResult {
    pub records: Vec<Record>,
    /// Rows skipped because the timestamp could not be parsed.
    pub bad_rows: usize,
}

/// Parse a PurpleAir CSV from any reader.
pub fn read_csv<R: Read>(reader: R) -> Result<ReadResult, String> {
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(reader);
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| format!("cannot read header row: {e}"))?
        .iter()
        .map(|h| h.trim().trim_start_matches('\u{feff}').to_ascii_lowercase())
        .collect();
    let t = find(&headers, TIME_COLS)
        .ok_or("no timestamp column (expected UTCDateTime or time_stamp)")?;
    let a = find(&headers, A_COLS)
        .ok_or("no channel A PM2.5 ATM column (expected pm2_5_atm or pm2.5_atm_a)")?;
    let b = find(&headers, B_COLS).ok_or(
        "no channel B PM2.5 ATM column (expected pm2_5_atm_b or pm2.5_atm_b); \
         single-channel indoor sensors are not supported",
    )?;
    let rh = find(&headers, RH_COLS)
        .ok_or("no humidity column (expected current_humidity or humidity)")?;

    let mut out = ReadResult::default();
    // Byte records: a row with corrupt (non-UTF-8) bytes, common on SD cards
    // after power loss, is skipped instead of aborting the whole file.
    for row in rdr.byte_records() {
        let row = row.map_err(|e| format!("CSV error: {e}"))?;
        let Ok(row) = csv::StringRecord::from_byte_record(row) else {
            out.bad_rows += 1;
            continue;
        };
        // SD cards sometimes contain a repeated header after a reboot.
        let Some(ts) = row.get(t).and_then(parse_timestamp) else {
            out.bad_rows += 1;
            continue;
        };
        out.records.push(Record {
            ts,
            a: num(row.get(a)),
            b: num(row.get(b)),
            rh: num(row.get(rh)).filter(|v| (0.0..=100.0).contains(v)),
        });
    }
    Ok(out)
}

/// Merge records from several files: sort by time and drop duplicate
/// timestamps (keeping the first seen).
pub fn merge(mut records: Vec<Record>) -> Vec<Record> {
    records.sort_by_key(|r| r.ts);
    records.dedup_by_key(|r| r.ts);
    records
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sd_card_columns() {
        let csv = "UTCDateTime,mac_address,current_temp_f,current_humidity,pm2_5_cf_1,pm2_5_atm,pm2_5_cf_1_b,pm2_5_atm_b\n\
                   2024/07/01T00:00:00z,8:3a:8d:b:a4:fd,70,45,12,11.5,13,12.5\n";
        let r = read_csv(csv.as_bytes()).unwrap();
        assert_eq!(r.records.len(), 1);
        let rec = r.records[0];
        assert_eq!(rec.a, Some(11.5));
        assert_eq!(rec.b, Some(12.5));
        assert_eq!(rec.rh, Some(45.0));
    }

    #[test]
    fn reads_api_columns() {
        let csv = "time_stamp,sensor_index,humidity,pm2.5_atm_a,pm2.5_atm_b\n\
                   1719792000,12345,40,8,9\n";
        let r = read_csv(csv.as_bytes()).unwrap();
        assert_eq!(r.records[0].ts, 1_719_792_000);
        assert_eq!(r.records[0].a, Some(8.0));
    }

    #[test]
    fn skips_repeated_header_and_handles_blanks() {
        let csv = "UTCDateTime,current_humidity,pm2_5_atm,pm2_5_atm_b\n\
                   UTCDateTime,current_humidity,pm2_5_atm,pm2_5_atm_b\n\
                   2024/07/01T00:00:00z,,nan,3\n\
                   2024/07/01T00:02:00z,120,1,2,extra\n";
        let r = read_csv(csv.as_bytes()).unwrap();
        assert_eq!(r.bad_rows, 1);
        assert_eq!(r.records.len(), 2);
        assert_eq!(r.records[0].a, None);
        assert_eq!(r.records[0].rh, None);
        assert_eq!(r.records[1].rh, None); // 120 % is out of range
    }

    #[test]
    fn corrupt_bytes_skip_only_that_row() {
        // SD cards written during power loss can contain invalid UTF-8.
        let mut csv = b"UTCDateTime,current_humidity,pm2_5_atm,pm2_5_atm_b\n\
                        2024/07/01T00:00:00z,40,1,2\n"
            .to_vec();
        csv.extend_from_slice(b"\xff\xfe\x00\x00garbage,\xff\n");
        csv.extend_from_slice(b"2024/07/01T00:04:00z,40,1,2\n");
        let r = read_csv(&csv[..]).unwrap();
        assert_eq!(r.records.len(), 2);
        assert_eq!(r.bad_rows, 1);
    }

    #[test]
    fn missing_b_channel_is_an_error() {
        let csv = "time_stamp,humidity,pm2.5_atm_a\n1,2,3\n";
        let e = read_csv(csv.as_bytes()).unwrap_err();
        assert!(e.contains("channel B"));
    }

    #[test]
    fn merge_sorts_and_dedups() {
        let r = |ts| Record {
            ts,
            a: Some(1.0),
            b: Some(1.0),
            rh: Some(1.0),
        };
        let m = merge(vec![r(3), r(1), r(3), r(2)]);
        assert_eq!(m.iter().map(|r| r.ts).collect::<Vec<_>>(), vec![1, 2, 3]);
    }
}
