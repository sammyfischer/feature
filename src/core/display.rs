//! Helper functions to display formatted strings. Diff-realted display
//! functions can be found in [super::diff]

use std::fmt::Display;

use anyhow::{Context, Result, anyhow};
use chrono::{FixedOffset, TimeZone};
use console::style;
use git2::{Commit, Object, Signature, Tag, Time};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::string::ToStrLossy;
use crate::core::trim_hash;

// Creates a [StyledObject] with format args
#[macro_export]
macro_rules! style {
  ($($arg:tt)*) => {
    console::style(&format!($($arg)*))
  };
}

/// Displays a trimmed hash in yellow
pub fn display_hash(obj: &Object) -> Result<String> {
  Ok(style(trim_hash(obj)?).yellow().to_string())
}

/// Displays the name in cyan, email in dim (gray), and "No user info" in red
/// if there is no configured signature.
pub fn display_signature(signature: Option<&Signature>) -> String {
  match signature {
    Some(it) => {
      let name = it.name_bytes().to_str_lossy();
      let email = it.email_bytes().to_str_lossy();
      format!("{} {}", style(name).cyan(), style(email).dim())
    }
    None => style("No user info").red().to_string(),
  }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisplayCommitOptions {
  pub time: DisplayTimeOptions,
  pub message: DisplayCommitMessageLevel,
}

#[derive(
  Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum DisplayCommitMessageLevel {
  None,
  Subject,
  #[default]
  Full,
}

impl Display for DisplayCommitMessageLevel {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      DisplayCommitMessageLevel::None => write!(f, "none"),
      DisplayCommitMessageLevel::Subject => write!(f, "subject"),
      DisplayCommitMessageLevel::Full => write!(f, "full"),
    }
  }
}

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

/// Displays formatted info about a commit
///
/// Example:
/// ```txt
/// 1234567 Apr 14, 2025 at 5:46 PM by Author Name
///
///   subject
///
///   body
/// ```
pub fn display_commit(commit: &Commit, options: &DisplayCommitOptions) -> Result<String> {
  use std::fmt::Write;
  // around 60 chars for hash/time/author, another 80 for message (most of the
  // time this will only be a subject line)
  let mut out = String::with_capacity(140);

  // hash
  write!(out, "{}", display_hash(commit.as_object())?)?;

  // timestamp
  write!(
    out,
    " {}",
    style(display_time(&commit.time(), &options.time)?).magenta()
  )?;

  // author
  let author = commit.author();
  let committer = commit.committer();
  write!(out, " by {}", display_signature(Some(&commit.author())))?;

  if author.name_bytes() != committer.name_bytes() {
    write!(
      out,
      "\n  {} {}",
      style("Committed by").dim(),
      style(display_signature(Some(&commit.committer()))).dim()
    )?;
  }

  match options.message {
    DisplayCommitMessageLevel::None => {}

    DisplayCommitMessageLevel::Subject => write!(
      out,
      "\n\n  {}",
      commit
        .summary_bytes()
        .context("Failed to get commit subject")?
        .to_str_lossy()
    )?,

    DisplayCommitMessageLevel::Full => {
      // write each line tabbed by 2 spaces
      writeln!(out)?;
      for line in commit.message_bytes().to_str_lossy().lines() {
        write!(out, "\n  {}", line)?;
      }
    }
  };

  Ok(out)
}

/// Displays a tag object in a format similar to a commit. Reuses
/// [DisplayCommitOptions] for convenience.
pub fn display_tag(tag: &Tag, options: &DisplayCommitOptions) -> Result<String> {
  use std::fmt::Write;
  // around 60 chars for hash/time/author, another 80 for message (most of the
  // time this will only be a subject line)
  let mut out = String::with_capacity(140);

  // hash
  write!(out, "{}", display_hash(tag.as_object())?)?;

  if let Some(tagger) = tag.tagger().as_ref() {
    // timestamp
    write!(
      out,
      " {}",
      style(display_time(&tagger.when(), &options.time)?).magenta()
    )?;

    // author
    write!(out, " by {}", display_signature(Some(tagger)))?;
  }

  if let Some(msg) = tag.message_bytes() {
    match options.message {
      DisplayCommitMessageLevel::None => {}

      // there is no subject line for tags. could just parse it out myself but I'd rather not
      // support it if it's non standard
      DisplayCommitMessageLevel::Subject | DisplayCommitMessageLevel::Full => {
        // write each line tabbed by 2 spaces
        writeln!(out)?;
        for line in msg.to_str_lossy().lines() {
          write!(out, "\n  {}", line)?;
        }
      }
    };
  }

  Ok(out)
}

/// A very concise format meant to be displayed on one line (although not
/// guaranteed to be). Unlike, [display_commit], there are no configuration
/// options.
///
/// ```txt
/// abcd123 (Author Name, 5 minutes ago) implemented change
/// ```
///
/// The hash is yellow, the parenthesized author/time is dim white (so just
/// gray) and the subject line is white.
pub fn display_commit_compact(commit: &Commit) -> Result<String> {
  Ok(format!(
    "{} {} {}",
    display_hash(commit.as_object())?,
    style(&format!(
      "({}, {})",
      commit.author().name_bytes().to_str_lossy(),
      display_time_relative(&commit.time())?
    ))
    .dim(),
    commit
      .summary_bytes()
      .expect("Commit should have a summary")
      .to_str_lossy()
  ))
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

/// Displays two numbers like `+p -m` where the first part is green and the
/// second part is red.
///
/// This is used to print ahead/behind and insertions/deletions.
pub fn display_plus_minus(plus: usize, minus: usize) -> String {
  format!(
    "{} {}",
    style(format!("+{}", plus)).green(),
    style(format!("-{}", minus)).red()
  )
}
