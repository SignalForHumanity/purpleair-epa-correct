# purpleair-epa-correct

Apply the U.S. EPA's quality control and **extended U.S. correction** (the one
used on the AirNow Fire and Smoke Map) to PurpleAir CSV files. The tool
produces hourly and daily PM2.5 with AQI values on the May 2024 breakpoints.

It is a single offline binary. You don't need R, Python, an API key or an
account, and your data never leaves your computer.

Repository: https://github.com/SignalForHumanity/purpleair-epa-correct ·
Background and research: [PROBLEM.md](PROBLEM.md)

## The problem

Raw PurpleAir readings overstate PM2.5, often by 40–60 %. The EPA publishes a
fix in several steps: average to hours, drop hours with too few readings,
drop hours where the sensor's two laser counters (A and B) disagree, apply a
5-piece humidity-adjusted equation, then compute AQI. The PurpleAir map does
this on screen. The API does not, and SD-card data from offline sensors never
reaches the map. Community groups end up rebuilding the workflow in
spreadsheets, often with a simplified formula or the old AQI breakpoints
([forum request](https://community.purpleair.com/t/epa-5-piece-correction-script/6276),
[API question](https://community.purpleair.com/t/is-there-a-field-that-returns-data-with-us-epa-pm2-5-conversion-formula-applied/4593)).

## Who it's for

Community air-monitoring groups, tribal air programs, neighborhood
associations, schools and libraries running PurpleAir sensors. It is
especially useful for anyone who pulls SD cards from offline sensors.

## Install

With Rust installed:

```sh
cargo install --git https://github.com/SignalForHumanity/purpleair-epa-correct
```

Or build from a checkout with `cargo build --release`. The binary is
`target/release/purpleair-epa-correct`. Copy it anywhere; it has no runtime
dependencies.

## Usage

```sh
# Summary only
purpleair-epa-correct sdcard/*.csv

# Write daily and hourly tables for Excel, with local time (UTC-7)
purpleair-epa-correct sdcard/*.csv --utc-offset -07:00 \
    --daily daily.csv --hourly hourly.csv
```

Example summary:

```
PurpleAir PM2.5, EPA-corrected (extended U.S. correction)
Files: 2  Readings: 1430  Unreadable rows: 0  Sampling interval: 120 s
Period (UTC+00:00): 2024-07-01 00:00 to 2024-07-02 23:59
Hours: 48 total, 46 valid, 1 incomplete, 1 A/B channel disagreement
Days: 2 with data, 2 complete enough for a daily mean

Daily AQI (24-hour mean, May 2024 breakpoints):
  Good                                1 day(s)
  Unhealthy                           1 day(s)

Mean of daily means: 53.0 µg/m³
Highest day: 2024-07-02 at 98.1 µg/m³
Days with a mean above 35.4 µg/m³ (AQI over 100): 1
```

Options:

| Option | Meaning |
|---|---|
| `-o, --hourly PATH` | hourly CSV (`-` for stdout) |
| `-d, --daily PATH` | daily CSV (`-` for stdout) |
| `-z, --utc-offset ±HH:MM` | fixed local offset for hours and days (default `+00:00`) |
| `--min-completeness PCT` | minimum % of expected readings per hour (default 90, as EPA) |
| `--min-day-hours N` | minimum valid hours for a daily mean (default 18 = 75 %) |
| `-q, --quiet` | no summary |

### Input files

The tool detects columns by header name and merges files, dropping duplicate
timestamps:

- **SD card** files: `UTCDateTime`, `pm2_5_atm`, `pm2_5_atm_b`, `current_humidity`.
- **API / PurpleAir Data Download Tool** exports: `time_stamp` (ISO or Unix),
  `pm2.5_atm_a`, `pm2.5_atm_b`, `humidity`. Request those fields when
  downloading. Hourly-average exports work too: each row counts as one
  complete hour.

ISO timestamps without an offset, or with `Z`/`UTC`, are read as UTC; a
numeric offset such as `-07:00` is applied. Rows that cannot be read (bad
timestamp, corrupt bytes) are skipped and counted as "Unreadable rows".

### Output columns

Hourly: `local_hour_start, samples, expected_samples, pm25_atm_a,
pm25_atm_b, rh, pm25_raw, pm25_corrected, flag, nowcast, nowcast_aqi,
nowcast_category`. `flag` is `valid`, `incomplete` or `ab_disagree`.
Only valid hours get a corrected value.

Daily: `local_date, valid_hours, complete, pm25_corrected_mean,
pm25_max_hour, aqi, category`. AQI is given only for complete days.

## Method

1. Each reading needs A, B and RH. RH outside 0–100 % counts as missing.
2. Readings are averaged into local clock hours. The expected count comes
   from the median sampling interval (30 per hour for 2-minute SD data).
3. An hour is **incomplete** if it has fewer than 90 % of the expected
   readings.
4. An hour is rejected as **ab_disagree** if the A and B hourly means differ
   by more than 5 µg/m³ **and** by more than 70 % (|A−B| ÷ mean(A,B)).
5. The EPA extended correction is applied to x = mean(A, B) of `pm2.5_atm`,
   using the sensor's RH:
   - x < 30: `0.524x − 0.0862RH + 5.75`
   - 30 ≤ x < 50: blend of the 0.524 and 0.786 slopes
   - 50 ≤ x < 210: `0.786x − 0.0862RH + 5.75`
   - 210 ≤ x < 260: blend toward the high-concentration curve
   - x ≥ 260: `2.966 + 0.69x + 8.84×10⁻⁴x²`

   Negative results are set to 0.
6. NowCast uses the 12-hour PM method and needs 2 of the last 3 hours.
7. Daily means need at least 18 valid hours. AQI uses the concentration
   truncated to 0.1 µg/m³ and the breakpoints in effect since May 2024
   (Good ≤ 9.0, Moderate ≤ 35.4, USG ≤ 55.4, Unhealthy ≤ 125.4,
   Very Unhealthy ≤ 225.4, Hazardous ≤ 325.4). Concentrations above
   325.4 µg/m³ are reported as AQI 500 (beyond the index). Daily means,
   daily maxima and NowCast values are shown truncated to 0.1 µg/m³, the
   same value the AQI is computed from.

## Data sources

- Barkjohn et al. (2022), "Correction and Accuracy of PurpleAir PM2.5
  Measurements for Extreme Wildfire Smoke", *Sensors* 22(24):9669.
  https://www.mdpi.com/1424-8220/22/24/9669
- EPA workflow for PurpleAir QC (90 % completeness; 5 µg/m³ and 70 % A/B
  check): https://cfpub.epa.gov/si/si_public_file_download.cfm?Lab=CEMM&p_download_id=539905
- PM2.5 AQI breakpoints, 2024 PM NAAQS rule:
  https://www.epa.gov/system/files/documents/2024-02/pm-naaqs-air-quality-index-fact-sheet.pdf
- PurpleAir SD card columns: https://community.purpleair.com/t/sd-card-file-headers/279

## Limitations

- **Outdoor dual-channel sensors only.** Single-channel indoor sensors (no B
  channel) are rejected. The indoor CF=1 correction is not implemented.
- The UTC offset is fixed. Daylight saving time is not applied, so for data
  that crosses a DST change, pick one offset or split the files.
- The tool does not remove readings after uptime resets on offline sensors,
  which EPA suggests. Check your SD data for long Wi-Fi search periods.
- The correction is a national average. It can under-read some dust events
  and wintertime pollution (Jaffe et al. 2023, AMT 16:1311). A local
  collocation study is still the gold standard.
- Corrected sensor data is **not regulatory data**. "Days above 35.4 µg/m³"
  is a useful indicator, but NAAQS compliance is judged on multi-year
  statistics from reference monitors.

## Development

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

The tests need no network. `tests/data/` holds synthetic SD-card files.

## License

MIT OR Apache-2.0, at your option.
