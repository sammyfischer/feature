use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct VersionConfig {
  /// Require version tags to be annotated
  pub require_annotated: bool,

  /// The pattern to match against version tags
  pub pattern: String,
}

impl Default for VersionConfig {
  fn default() -> Self {
    Self {
      require_annotated: false,
      pattern: "v*.*.*".to_string(),
    }
  }
}
