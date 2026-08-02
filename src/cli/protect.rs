use anyhow::{Context, Result};

use crate::App;
use crate::cli::advice::NOT_ON_BRANCH_MSG;
use crate::core::branch::get_current_branch_name;
use crate::core::user_config::{set_feature_protect, unset_feature_protect};

const LONG_ABOUT: &str = r#"Protects a branch from pruning.

Sets "feature-protect" in a branch's config to true. Feature respects this value
when pruning branches.

This not only prevents deletions, but also does so silently. This can be used to
suppress the skip messages when running prune or sync."#;

#[derive(clap::Args, Clone, Debug)]
#[command(
  about = "Protects a branch from being pruned",
  long_about = LONG_ABOUT,
  disable_help_subcommand = true
)]
pub struct ProtectArgs {
  /// Unset the config value (i.e. stop protecting the branch)
  #[arg(short, long)]
  unset: bool,

  /// The name of the branch to protect. Defaults to the current branch.
  branch: Option<String>,
}

impl ProtectArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo = &state.repo;
    let mut config = repo.config()?;

    let branch = match &self.branch {
      Some(name) => name,
      None => &get_current_branch_name(repo)?.context(NOT_ON_BRANCH_MSG)?,
    };

    if self.unset {
      unset_feature_protect(&mut config, branch)
    } else {
      set_feature_protect(&mut config, branch)
    }
  }
}
