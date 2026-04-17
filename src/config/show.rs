use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::PageWhen;
use crate::util::display::DisplayCommitMessageLevel;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ShowConfig {
  /// How much of the commit message to show
  pub message: DisplayCommitMessageLevel,

  /// Whether to show the diff summary
  pub summary: bool,

  /// Whether to show the diff patch
  pub patch: bool,

  /// When to send output to a pager
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
