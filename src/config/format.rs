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
      timezone: Default::default(),
    }
  }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DateStyle {
  #[default]
  Textual,
  Numeric,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum HourStyle {
  #[default]
  #[serde(rename = "12")]
  Twelve,
  #[serde(rename = "24")]
  TwentyFour,
}
