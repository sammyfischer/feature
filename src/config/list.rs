use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListConfig {
  pub hash: bool,
  pub upstream: bool,
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
