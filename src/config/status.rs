use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct StatusConfig {
  /// Whether to show untracked files in output
  pub show_untracked: bool,
}

impl Default for StatusConfig {
  fn default() -> Self {
    Self {
      show_untracked: true,
    }
  }
}
