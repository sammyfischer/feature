use anyhow::{Context, Result, anyhow};
use chrono::{FixedOffset, TimeZone};
use git2::Time;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayTimeOptions {
  /// False for absolute, true for relative
  pub relative: bool,
  pub fmt: String,
}

impl Default for DisplayTimeOptions {
  fn default() -> Self {
    Self {
      relative: Default::default(),
      fmt: "%b %d, %Y at %I:%M %p".to_string(),
    }
  }
}

/// Displays a human readable time
pub fn display_time(time: &Time, options: &DisplayTimeOptions) -> Result<String> {
  if options.relative {
    display_time_relative(time)
  } else {
    display_time_absolute(time, &options.fmt)
  }
}

/// `fmt` is passed to chronos as-is. Defaults to "%b %d, %Y at %I:%M %p", which
/// looks like "May 11, 2026 at 4:44 PM"
pub fn display_time_absolute(time: &Time, fmt: &str) -> Result<String> {
  let tz = FixedOffset::east_opt(time.offset_minutes() * 60)
    .ok_or(anyhow!("Failed to format time to local timezone"))?;

  let date = tz
    .timestamp_opt(time.seconds(), 0)
    .single()
    .ok_or(anyhow!("Failed to format time to local timezone"))?;

  Ok(date.format(fmt).to_string())
}

pub fn display_time_relative(time: &Time) -> Result<String> {
  let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .context("Failed to get current time")?
    .as_secs() as i64;

  let secs = now - time.seconds();

  const HOUR: i64 = 60 * 60;
  const DAY: i64 = HOUR * 24;
  const WEEK: i64 = DAY * 7;
  const MONTH: i64 = DAY * 30;
  const YEAR: i64 = DAY * 365;

  // this should roughly match git log's relative time format
  Ok(match secs {
    s if s < 2 => "1 second ago".to_string(),
    s if s < 60 => format!("{} seconds ago", s),

    s if s < 120 => "1 minute ago".to_string(),
    s if s < HOUR => format!("{} minutes ago", s / 60),

    s if s < HOUR * 2 => "1 hour ago".to_string(),
    s if s < DAY => format!("{} hours ago", s / HOUR),

    s if s < DAY * 2 => "yesterday".to_string(),
    s if s < WEEK => format!("{} days ago", s / DAY),

    s if s < WEEK * 2 => "1 week ago".to_string(),
    s if s < MONTH => format!("{} weeks ago", s / WEEK),

    s if s < MONTH * 2 => "1 month ago".to_string(),
    s if s < YEAR => format!("{} months ago", s / MONTH),

    s if s < YEAR * 2 => "1 year ago".to_string(),
    s => format!("{} years ago", s / YEAR),
  })
}
