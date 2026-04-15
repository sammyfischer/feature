//! Helper functions to display formatted strings. Diff-realted display functions can be found in
//! [super::diff]

use anyhow::{Context, Result, anyhow};
use chrono::{FixedOffset, TimeZone};
use console::style;
use git2::{Commit, Oid, Signature, Time};

use crate::lossy;

pub fn trim_hash(id: &Oid) -> String {
  id.to_string()[..7].to_string()
}

/// Displays a trimmed hash in yellow
pub fn display_hash(id: &Oid) -> String {
  style(trim_hash(id)).yellow().to_string()
}

/// Displays a human-readable absolute time
pub fn display_time_absolute(time: &Time) -> Result<String> {
  let timezone = FixedOffset::east_opt(time.offset_minutes() * 60)
    .ok_or(anyhow!("Failed to format time to local timezone"))?;

  let date = timezone
    .timestamp_opt(time.seconds(), 0)
    .single()
    .ok_or(anyhow!("Failed to format time to local timezone"))?;

  // TODO: 24 hour config option
  Ok(date.format("%B %d, %Y at %I:%M %p").to_string())
}

/// Displays a human-readable relative time
pub fn display_time_relative(time: &Time) -> Result<String> {
  let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .context("Failed to get current time")?
    .as_secs() as i64;

  let secs = now - time.seconds();

  const HOUR: i64 = 60 * 60;
  const DAY: i64 = HOUR * 24;
  const WEEK: i64 = DAY * 7;
  const MONTH: i64 = WEEK * 4;
  const YEAR: i64 = MONTH * 12;

  // this should match git log's relative time format
  Ok(match secs {
    s if s < 60 => "just now".to_string(),

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

/// Displays the name in cyan, email in dim (gray), and "no one" in red if there is no configured
/// signature. Errors if any error (other than not having a signature) is encountered.
pub fn display_signature(signature: Option<&Signature>) -> String {
  match signature {
    Some(it) => {
      let name = lossy!(it.name_bytes());
      let email = lossy!(it.email_bytes());
      format!("{} {}", style(name).cyan(), style(email).dim())
    }
    None => style("no one").red().to_string(),
  }
}

/// Displays full info about a commit
///
/// ```txt
/// 1234567 Apr 14, 2025 at 5:46 PM by Author Name
///
///   subject
///
///   body
/// ```
pub fn display_commit_full(commit: &Commit) -> Result<String> {
  use std::fmt::Write;
  // around 60 chars for hash/time/author, another 80 for message (most of the time this will only
  // be a subject line)
  let mut out = String::with_capacity(140);

  // hash
  write!(out, "{}", display_hash(&commit.id()))?;

  // timestamp (absolute)
  write!(
    out,
    " {}",
    style(display_time_absolute(&commit.time())?).magenta()
  )?;

  // author
  let author = commit.author();
  let committer = commit.committer();
  writeln!(out, " by {}", display_signature(Some(&commit.author())))?;

  if author.name_bytes() != committer.name_bytes() {
    writeln!(
      out,
      "  {} {}",
      style("Committed by").dim(),
      style(display_signature(Some(&commit.committer()))).dim()
    )?;
  }

  // write each line tabbed by 2 spaces
  for line in lossy!(commit.message_bytes()).lines() {
    write!(out, "\n  {}", line)?;
  }

  Ok(out)
}

/// Displays two numbers like `+p -m` where the first part is green and the second part is red.
///
/// This is used to print ahead/behind and insertions/deletions.
pub fn display_plus_minus(plus: usize, minus: usize) -> String {
  format!(
    "{} {}",
    style(format!("+{}", plus)).green(),
    style(format!("-{}", minus)).red()
  )
}
