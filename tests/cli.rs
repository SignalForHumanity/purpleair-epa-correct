//! End-to-end tests against synthetic SD-card files in tests/data.
//!
//! 20240701.csv: clean 10/11 µg/m³ at 40 % RH, except hour 05 where channel B
//! reads 60 (A/B disagreement) and hour 12 which has only 20 of 30 readings.
//! 20240702.csv: smoke, 120/124 µg/m³ all day.

use std::path::Path;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_purpleair-epa-correct"))
}

fn data(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn daily_table_to_stdout() {
    let out = bin()
        .args([data("20240702.csv"), data("20240701.csv")])
        .args(["--daily", "-", "--quiet"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let s = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = s.lines().collect();
    assert_eq!(lines.len(), 3, "{s}");
    assert_eq!(lines[1], "2024-07-01,22,true,7.8,7.8,43,Good");
    assert_eq!(lines[2], "2024-07-02,24,true,98.1,98.1,181,Unhealthy");
}

#[test]
fn hourly_flags_and_summary() {
    let dir = std::env::temp_dir().join(format!("pae-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let hourly = dir.join("hourly.csv");
    let out = bin()
        .args([data("20240701.csv"), data("20240702.csv")])
        .arg("--hourly")
        .arg(&hourly)
        .output()
        .unwrap();
    assert!(out.status.success());
    let summary = String::from_utf8(out.stdout).unwrap();
    assert!(summary.contains("Readings: 1430"), "{summary}");
    assert!(summary.contains("48 total, 46 valid, 1 incomplete, 1 A/B channel disagreement"));
    assert!(summary.contains("AQI over 100): 1"));

    let csv = std::fs::read_to_string(&hourly).unwrap();
    let rows: Vec<&str> = csv.lines().collect();
    assert_eq!(rows.len(), 49);
    assert!(rows[6].starts_with("2024-07-01 05:00,30,30,10.0,60.0,40.0,35.0,,ab_disagree"));
    assert!(rows[13].starts_with("2024-07-01 12:00,20,30,"));
    assert!(rows[13].contains(",incomplete,"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn utc_offset_moves_day_boundaries() {
    let out = bin()
        .args([data("20240701.csv"), data("20240702.csv")])
        .args(["-d", "-", "-q", "--utc-offset", "-07:00"])
        .output()
        .unwrap();
    let s = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = s.lines().collect();
    // Local days: Jun 30 (17:00–23:59, 7 h, one rejected), Jul 1, Jul 2 (00:00–16:59, 17 h).
    assert_eq!(lines.len(), 4, "{s}");
    assert!(lines[1].starts_with("2024-06-30,6,false,"));
    assert!(lines[3].starts_with("2024-07-02,17,false,"));
}

#[test]
fn bad_arguments_fail() {
    let out = bin().output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let out = bin()
        .args(["--utc-offset", "nope", "x.csv"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let out = bin().arg("/no/such/file.csv").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
}
