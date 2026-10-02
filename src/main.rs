use purpleair_epa_correct::{input, output, process, time};
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
purpleair-epa-correct: apply EPA's QC and extended correction to PurpleAir CSV files

USAGE:
    purpleair-epa-correct [OPTIONS] <FILE>...

FILES:
    PurpleAir CSVs: SD-card files (UTCDateTime, pm2_5_atm, pm2_5_atm_b,
    current_humidity) or API / Data Download Tool exports (time_stamp,
    pm2.5_atm_a, pm2.5_atm_b, humidity). Several files are merged.

OPTIONS:
    -o, --hourly <PATH>        Write hourly CSV (use - for stdout)
    -d, --daily <PATH>         Write daily CSV (use - for stdout)
    -z, --utc-offset <OFFSET>  Local offset for hours and days, e.g. -07:00
                               (default +00:00; fixed, no daylight saving)
        --min-completeness <PCT>
                               Minimum % of expected readings per hour (default 90)
        --min-day-hours <N>    Minimum valid hours for a daily mean (default 18)
    -q, --quiet                Do not print the summary
    -h, --help                 Show this help
    -V, --version              Show version
";

struct Args {
    files: Vec<PathBuf>,
    hourly: Option<String>,
    daily: Option<String>,
    opts: process::Options,
    quiet: bool,
}

fn parse_args(raw: Vec<String>) -> Result<Option<Args>, String> {
    let mut args = Args {
        files: Vec::new(),
        hourly: None,
        daily: None,
        opts: process::Options::default(),
        quiet: false,
    };
    let mut it = raw.into_iter();
    while let Some(a) = it.next() {
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
            _ => (a.clone(), None),
        };
        let mut value = |name: &str| -> Result<String, String> {
            inline
                .clone()
                .or_else(|| it.next())
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match flag.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("purpleair-epa-correct {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "-o" | "--hourly" => args.hourly = Some(value("--hourly")?),
            "-d" | "--daily" => args.daily = Some(value("--daily")?),
            "-z" | "--utc-offset" => {
                let v = value("--utc-offset")?;
                args.opts.utc_offset =
                    time::parse_offset(&v).ok_or_else(|| format!("invalid UTC offset: {v}"))?;
            }
            "--min-completeness" => {
                let v = value("--min-completeness")?;
                let p: f64 = v
                    .parse()
                    .ok()
                    .filter(|p| (0.0..=100.0).contains(p))
                    .ok_or_else(|| format!("invalid percentage: {v}"))?;
                args.opts.min_completeness = p / 100.0;
            }
            "--min-day-hours" => {
                let v = value("--min-day-hours")?;
                args.opts.min_day_hours = v
                    .parse()
                    .ok()
                    .filter(|n| (1..=24).contains(n))
                    .ok_or_else(|| format!("invalid hour count (1-24): {v}"))?;
            }
            "-q" | "--quiet" => args.quiet = true,
            s if s.starts_with('-') && s != "-" => return Err(format!("unknown option: {s}")),
            _ => args.files.push(PathBuf::from(a)),
        }
    }
    if args.files.is_empty() {
        return Err("no input files given".into());
    }
    Ok(Some(args))
}

fn open_out(path: &str) -> Result<Box<dyn Write>, String> {
    if path == "-" {
        Ok(Box::new(io::stdout().lock()))
    } else {
        let f = File::create(path).map_err(|e| format!("{path}: {e}"))?;
        Ok(Box::new(BufWriter::new(f)))
    }
}

fn run(args: Args) -> Result<(), String> {
    let mut all = Vec::new();
    let mut bad_rows = 0;
    for path in &args.files {
        let f = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let r =
            input::read_csv(BufReader::new(f)).map_err(|e| format!("{}: {e}", path.display()))?;
        bad_rows += r.bad_rows;
        all.extend(r.records);
    }
    let records = input::merge(all);
    if records.is_empty() {
        return Err("no readable rows in the input files".into());
    }
    let hours = process::hourly(&records, &args.opts);
    let days = process::daily(&hours, &args.opts);

    if let Some(p) = &args.hourly {
        output::write_hourly(open_out(p)?, &hours)?;
    }
    if let Some(p) = &args.daily {
        output::write_daily(open_out(p)?, &days)?;
    }
    if !args.quiet {
        let text = output::summary(&output::SummaryInput {
            files: args.files.len(),
            readings: records.len(),
            bad_rows,
            interval: process::sample_interval(&records),
            utc_offset: args.opts.utc_offset,
            hours: &hours,
            days: &days,
        });
        // Keep CSV on stdout clean if a table is going there.
        let to_stdout = args.hourly.as_deref() != Some("-") && args.daily.as_deref() != Some("-");
        if to_stdout {
            print!("{text}");
        } else {
            eprint!("{text}");
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match parse_args(std::env::args().skip(1).collect()) {
        Ok(None) => ExitCode::SUCCESS,
        Ok(Some(args)) => match run(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("error: {e}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
