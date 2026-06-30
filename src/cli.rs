use std::path::PathBuf;

use clap::Parser;

use crate::dates::{parse_date_spec_clap, DateSpec};

#[derive(Parser, Debug)]
#[command(version, about = "Download PCSO lotto results and archive them to S3")]
pub struct Args {
    /// Start date. Accepts `MM-dd-yyyy` (single day, e.g. `03-05-2026`) or
    /// `MonthName-yyyy` (e.g. `March-2026` → first day of that month).
    /// Defaults to today in Asia/Manila.
    #[arg(long, value_parser = parse_date_spec_clap)]
    pub from: Option<DateSpec>,

    /// End date, inclusive. Same formats as `--from`; `MonthName-yyyy`
    /// resolves to the last day of the month. Defaults to `--from`.
    #[arg(long, value_parser = parse_date_spec_clap)]
    pub to: Option<DateSpec>,

    /// Show the browser window (default: headless).
    #[arg(long)]
    pub headed: bool,

    /// When --headed, start with the window minimized (out of the way).
    #[arg(long, requires = "headed")]
    pub minimize: bool,

    /// S3 bucket to upload into. Required.
    #[arg(long)]
    pub bucket: String,

    /// AWS profile name (from ~/.aws/credentials). Overrides AWS_PROFILE / default chain.
    #[arg(long)]
    pub profile: Option<String>,

    /// AWS region of the target bucket.
    #[arg(long, default_value = "us-east-1")]
    pub region: String,

    /// Persistent Chromium profile directory. Defaults to `.pcso-profile`
    /// next to the binary; falls back to PCSO_PROFILE_DIR env var if set.
    #[arg(long)]
    pub profile_dir: Option<PathBuf>,
}
