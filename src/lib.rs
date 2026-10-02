//! Apply U.S. EPA quality control and the extended U.S. correction to
//! PurpleAir PM2.5 CSV files, and summarize hourly and daily PM2.5 and AQI.

pub mod aqi;
pub mod correction;
pub mod input;
pub mod output;
pub mod process;
pub mod time;
