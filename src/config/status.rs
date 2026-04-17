use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StatusConfig {
  pub show_untracked: bool,
}

impl Default for StatusConfig {
  fn default() -> Self {
    Self {
      show_untracked: true,
    }
  }
}
