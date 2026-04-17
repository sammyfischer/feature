use serde::{Deserialize, Serialize};

use crate::config::PageWhen;
use crate::util::display::DisplayCommitMessageLevel;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShowConfig {
  pub message: DisplayCommitMessageLevel,
  pub summary: bool,
  pub patch: bool,
  pub paging: PageWhen,
}

impl Default for ShowConfig {
  fn default() -> Self {
    Self {
      message: Default::default(),
      summary: true,
      patch: true,
      paging: Default::default(),
    }
  }
}
