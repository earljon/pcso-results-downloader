use aws_sdk_s3::{config::Region, error::ProvideErrorMetadata, primitives::ByteStream, Client};
use chrono::NaiveDate;
use tracing::info;

use crate::dates::{format_for_path, month_name};
use crate::error::{PcsoError, Result};

pub async fn client(profile: Option<&str>, region: &str) -> Client {
    let mut builder = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(Region::new(region.to_string()));
    if let Some(name) = profile {
        builder = builder.profile_name(name);
    }
    let config = builder.load().await;
    Client::new(&config)
}

pub fn key_for(date: NaiveDate) -> String {
    format!(
        "results/downloads/{year}/{month}/{date}.html",
        year = date.format("%Y"),
        month = month_name(date),
        date = format_for_path(date),
    )
}

pub async fn upload_html(client: &Client, bucket: &str, key: &str, html: String) -> Result<()> {
    let body = ByteStream::from(html.into_bytes());
    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(body)
        .content_type("text/html; charset=utf-8")
        .send()
        .await
        .map_err(|e| {
            let code = e.code().unwrap_or("unknown").to_string();
            let msg = e.message().unwrap_or("(no message)").to_string();
            PcsoError::S3 {
                key: key.to_string(),
                source: format!("{code}: {msg}").into(),
            }
        })?;
    info!("uploaded s3://{}/{}", bucket, key);
    Ok(())
}
