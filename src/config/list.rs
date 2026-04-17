use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ListConfig {
  /// Whether to show the hash column
  pub hash: bool,

  /// Whether to show the upstream column
  pub upstream: bool,

  /// Whether to show the base column
  pub base: bool,
}

impl Default for ListConfig {
  fn default() -> Self {
    Self {
      hash: true,
      upstream: true,
      base: true,
    }
  }
}
