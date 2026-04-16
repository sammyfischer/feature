use std::fmt::Display;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct FormatConfig {
  /// Separator used between words in branch names
  pub branch_sep: String,

  /// Template for creating branch names. See `feature start --help` for more info
  #[serde(skip_serializing_if = "Option::is_none")]
  pub branch: Option<String>,

  /// Template for log output
  pub log: String,

  /// Template for graph output
  pub graph: String,

  /// Hour format, 12 or 24 hour
  pub hour: HourStyle,

  /// Date format (respect hour format)
  /// Verbose: Apr 14, 2026 at 11:26 PM
  /// Compact: 2026-14-04 11:26 PM
  pub date: DateStyle,

  /// Whether to show timezone offset
  pub timezone: bool,

  /// Whether to show relative time instead of absolute
  pub relative: bool,
}

impl Default for FormatConfig {
  fn default() -> Self {
    Self {
      branch_sep: "-".into(),
      branch: Default::default(),
      log: "format:%C(auto)%h%d %C(reset)%s %C(dim)(%an, %ar)".into(),
      graph: "format:%C(auto)%h%d %C(green)%an %C(blue)%ar %C(reset)%s".into(),
      hour: Default::default(),
      date: Default::default(),
      timezone: false,
      relative: false,
    }
  }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DateStyle {
  #[default]
  Textual,
  Numeric,
}

impl Display for DateStyle {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", match self {
      Self::Textual => "textual",
      Self::Numeric => "numeric",
    })
  }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum HourStyle {
  #[default]
  #[serde(rename = "12")]
  Twelve,
  #[serde(rename = "24")]
  TwentyFour,
}

impl Display for HourStyle {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", match self {
      HourStyle::Twelve => "12",
      HourStyle::TwentyFour => "24",
    })
  }
}
