use std::time::Duration;

use aws_sdk_s3::Client as S3Client;
use chrono::NaiveDate;
use chromiumoxide::Browser;
use tokio::time::sleep;
use tracing::{error, info, warn};

use crate::browser::fetch_result_html;
use crate::cli::Args;
use crate::dates::{format_for_path, range_inclusive, today_manila, DateSpec};
use crate::error::{PcsoError, Result};
use crate::s3::{key_for, upload_html};

const MAX_ATTEMPTS: u32 = 3;

pub async fn run(args: Args) -> Result<()> {
    let dates = build_date_list(&args)?;
    info!("preparing to process {} date(s)", dates.len());

    let profile = crate::browser::resolve_profile_dir(args.profile_dir.clone());
    info!("using profile dir {}", profile.display());
    let (mut browser, _handler) =
        crate::browser::launch(args.headed, args.minimize, profile).await?;
    let s3 = crate::s3::client(args.profile.as_deref(), &args.region).await;

    let total = dates.len();
    for (idx, date) in dates.iter().copied().enumerate() {
        let label = format_for_path(date);
        info!("[{}/{}] processing {}", idx + 1, total, label);
        if let Err(e) = process_date(&browser, &s3, &args.bucket, date).await {
            error!("{}", e);
            // best effort to close chrome before bubbling
            let _ = browser.close().await;
            return Err(e);
        }
    }

    let _ = browser.close().await;
    info!("done — {} date(s) uploaded to s3://{}/", total, args.bucket);
    Ok(())
}

fn build_date_list(args: &Args) -> Result<Vec<NaiveDate>> {
    // --from default is today (a single day). --to default is whatever --from
    // resolves to (so `--from March-2026` alone gives March 1..31, and
    // `--from 03-05-2026` alone gives just March 5).
    let from_spec = args.from.unwrap_or_else(|| DateSpec::Day(today_manila()));
    let to_spec = args.to.unwrap_or(from_spec);
    let from = from_spec.first_day();
    let to = to_spec.last_day();
    range_inclusive(from, to)
}

async fn process_date(
    browser: &Browser,
    s3: &S3Client,
    bucket: &str,
    date: NaiveDate,
) -> Result<()> {
    let label = format_for_path(date);
    let key = key_for(date);

    let mut last_err: Option<PcsoError> = None;
    for attempt in 1..=MAX_ATTEMPTS {
        match attempt_once(browser, s3, bucket, date, &key).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                warn!(
                    "attempt {}/{} for {} failed: {}",
                    attempt, MAX_ATTEMPTS, label, e
                );
                last_err = Some(e);
                if attempt < MAX_ATTEMPTS {
                    let backoff = Duration::from_secs(1u64 << attempt); // 2s, 4s
                    sleep(backoff).await;
                }
            }
        }
    }

    let source = last_err.expect("loop ran at least once");
    Err(PcsoError::DateAborted {
        date: label,
        attempts: MAX_ATTEMPTS,
        source: Box::new(source),
    })
}

async fn attempt_once(
    browser: &Browser,
    s3: &S3Client,
    bucket: &str,
    date: NaiveDate,
    key: &str,
) -> Result<()> {
    let html = fetch_result_html(browser, date).await?;
    if html.trim().is_empty() {
        return Err(PcsoError::Browser("page returned empty HTML".into()));
    }
    upload_html(s3, bucket, key, html).await?;
    Ok(())
}
