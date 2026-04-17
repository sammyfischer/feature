use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdviceConfig {
  /// Advice on staging/unstaging
  pub status: bool,

  /// Advice on rebase conflicts
  pub rebase: bool,

  /// Advice on merge conflicts
  pub merge: bool,

  /// Advice on cherry-pick conflicts
  pub cherry_pick: bool,

  /// Advice on revert conflicts
  pub revert: bool,

  /// Advice on bisect
  pub bisect: bool,
}

impl Default for AdviceConfig {
  fn default() -> Self {
    Self {
      // false bc people generally know how to stage/unstage
      status: false,
      rebase: true,
      merge: true,
      cherry_pick: true,
      revert: true,
      // false bc bisect is a state you enter intentionally
      bisect: false,
    }
  }
}
