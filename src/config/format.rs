use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct FormatConfig {
  /// Separator used between words in branch names
  pub branch_sep: String,

  /// Template for creating branch names. See `feature start --help` for more info
  #[serde(skip_serializing_if = "Option::is_none")]
  pub branch: Option<String>,
}

impl Default for FormatConfig {
  fn default() -> Self {
    Self {
      branch_sep: "-".into(),
      branch: Default::default(),
    }
  }
}
