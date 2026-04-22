use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SyncConfig {
  /// Whether to prune after syncing
  pub prune: bool,
}

impl Default for SyncConfig {
  fn default() -> Self {
    Self { prune: true }
  }
}
