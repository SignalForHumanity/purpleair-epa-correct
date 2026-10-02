---
slug: purpleair-epa-correct
title: CLI that applies EPA's QC and extended correction to PurpleAir CSV files and reports daily PM2.5 and AQI
verdict: build
---

## Problem

Community groups, tribal environmental offices and neighborhood associations
run PurpleAir PM2.5 sensors because regulatory monitors are far away. Raw
PurpleAir readings overstate PM2.5 by roughly 40–60 % (EPA, "PurpleAir PM2.5
performance across the U.S.", https://cfpub.epa.gov/si/si_public_record_report.cfm?Lab=CEMM&dirEntryId=348236).
EPA's fix is a published workflow: average 2-minute data to hours, drop hours
under 90 % complete, drop hours where channels A and B disagree, apply the
extended 5-piece U.S. correction (used on the AirNow Fire and Smoke Map), then
compute daily means and AQI.

Groups that work from downloaded or SD-card CSV files have to do all of that
by hand. They keep asking for it:

- "Does anyone have or know of someone using an R script to automatically
  apply the EPA correction equation to PurpleAir data? … all 5 equations"
  and the reply "I too would love this, maybe even as an excel macro book!?"
  (https://community.purpleair.com/t/epa-5-piece-correction-script/6276)
- The PurpleAir API returns no corrected field. Users must "manually apply the
  correction formula" and then cannot reproduce the map's numbers
  (https://community.purpleair.com/t/is-there-a-field-that-returns-data-with-us-epa-pm2-5-conversion-formula-applied/4593,
  2,300+ views).
- A Rochester, MN community network asking how to correct and analyze its
  data (https://community.purpleair.com/t/correction-factor-question/7411).
  In the same thread a residents' group (RAWSE) says it uses Excel templates
  with a simplified linear formula to compare 3-day averages to the NAAQS.
- A September 2026 issue asking for the 5-piece correction in a local-sensor
  integration (https://github.com/jpettitt/purpleair-local/issues/15).

Hand-built spreadsheets get the piecewise blending wrong, skip the A/B check,
or use the pre-May-2024 AQI breakpoints. The result is numbers that are
inconsistent and easy to dismiss when a group brings them to an air district
or a city council.

## Who benefits

- Volunteer and community air-monitoring groups, tribal air programs (ITEP
  trains them on PurpleAir downloads:
  https://itep.nau.edu/wp-content/uploads/2025/06/Download_Interpret_Purple_Air_Data.pdf),
  and school or library sensor projects.
- Most of all, people with offline sensors who pull SD cards. Their data
  never reaches the PurpleAir map, so the map's conversion never applies to it.

They would find it through the PurpleAir community forum, EPA Air Sensor
Toolbox lists such as the Awesome Air Quality list, and GitHub search. They
run it by downloading one binary and pointing it at their CSV files. It
writes CSVs they can open in Excel.

## Existing solutions

- **PurpleAir map "US EPA" conversion** (https://map.purpleair.com): display
  only. It covers online sensors only, and the API has no corrected field.
- **PurpleAir Data Download Tool**
  (https://community.purpleair.com/t/purpleair-data-download-tool/3787):
  downloads raw API fields and needs API keys with history access. It applies
  no correction.
- **EPA ASNAT** (https://www.epa.gov/air-sensor-toolbox/air-sensor-data-tools):
  free R Shiny app. It needs R installed and EPA says its "target audience is
  air quality professionals". It works from RSIG public data and SD files.
- **EPA ASDU**: R Shiny reformatter. It does not do the full correction and
  AQI workflow.
- **EPA sensortoolkit** (https://github.com/USEPA/sensortoolkit): Python
  library for collocation evaluation against reference monitors. It needs
  programming skill.
- **SebAire/Purple-Air-Data-Merger**
  (https://github.com/SebAire/Purple-Air-Data-Merger, 6 stars): R Shiny, SD
  card only. It needs R and RStudio set up and uses the simple correction
  plus an optional smoke switch, not the extended 5-piece equation. No AQI or
  daily summary.
- **MazamaScience/AirSensor** (https://github.com/MazamaScience/AirSensor):
  R package, last push 2023, built around the old PurpleAir API.
- **oliviasablan/PurpleAir_data_wrangling**: Jupyter notebooks for one
  research workflow.
- **Crates**: `aqi` (last update 2022, before the 2024 breakpoints). No
  crate does PurpleAir QC or correction.
- **Spreadsheets** (RAWSE Excel templates, ITEP timestamp spreadsheet):
  manual, and they use simplified formulas.

## Why build anything

Every complete existing option needs R or Python installed and someone who
can run it. EPA itself aims ASNAT at professionals. The one turnkey option,
the PurpleAir map, does not cover SD-card data or bulk CSV exports in a form
people can analyze. Volunteers are left with spreadsheets and simplified
formulas. The gap is narrow but real: a no-install, single-file tool that
runs EPA's exact published workflow on the CSV files people already have.

## Smallest useful intervention

`purpleair-epa-correct`, a single static binary:

- Reads one or more PurpleAir CSVs in SD-card format or API / Data Download
  Tool format, merges them and drops duplicate timestamps.
- Averages to hours and applies EPA QC: completeness of at least 90 %, and
  drops an hour when A and B differ by more than 5 µg/m³ AND more than 70 %.
- Applies the EPA extended 5-piece U.S. correction (Barkjohn et al. 2022) to
  the A/B-averaged ATM value with the sensor's RH.
- Writes an hourly CSV (with NowCast and NowCast AQI) and a daily CSV (24-hour
  mean, AQI on the May 2024 breakpoints, category). Daily means use a fixed
  local UTC offset.
- Prints a plain summary: valid hours, days per AQI category, and days whose
  mean is above 35 µg/m³, with the caveat that sensor data is not regulatory.

## Success criterion

- Unit tests reproduce the published equation at every piece boundary, and
  the curve is continuous at 30, 50, 210 and 260 µg/m³.
- The AQI matches EPA's 2024 breakpoint table at every boundary.
- A synthetic 2-minute SD-card fixture yields the expected hourly flags and
  daily means.
- A volunteer can go from a folder of SD-card files to a daily CSV with one
  command, with no installs beyond the binary.

## Maintenance

It is offline and has no server or API. It breaks only if PurpleAir renames
CSV columns (a header alias table makes that easy to fix) or if EPA publishes
a new correction or new AQI breakpoints (constants in one file). Expected
upkeep is about an hour a year. The only dependency is `csv`.

## Decision

Build. The demand is sourced and recent. The existing tools need R or Python
and professional knowledge, and the map conversion skips SD-card data. A
narrow offline CLI that implements EPA's published workflow exactly fills
the gap. Data stays on the user's machine, no personal data is involved, and
it costs nothing to run.

## Also considered

- food-bank / volunteer-shift tooling from r/nonprofit threads: generic CRM/scheduling needs already met by Food Pantry Helper, PantrySoft, VolunteerMark, spreadsheets and TechSoup-discounted software.
