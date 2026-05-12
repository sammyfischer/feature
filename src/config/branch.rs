use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct BranchConfig {
  /// Separator used between words in branch names
  pub sep: String,

  /// Template for creating branch names. See `feature start --help` for more info
  #[serde(skip_serializing_if = "Option::is_none")]
  pub template: Option<String>,
}

impl Default for BranchConfig {
  fn default() -> Self {
    Self {
      sep: "-".into(),
      template: Default::default(),
    }
  }
}
