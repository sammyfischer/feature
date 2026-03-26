//! Interactions with persistent data

use git2::{Config, Repository};

use crate::cli::CliResult;
use crate::cli_err_fn;

/// Gets the git config and converts errors into `CliError`s
pub fn git_config(repo: &Repository) -> CliResult<Config> {
  repo
    .config()
    .map_err(cli_err_fn!(Git, e, "Failed to get git config: {e}"))
}

/// Gets the feature-base of a branch. If not found, defaults to vscode-merge-base.
pub fn get_feature_base(config: &Config, branch: &str) -> Option<String> {
  let base = config
    .get_string(&format!("branch.{}.feature-base", &branch))
    .ok()?;

  if !base.is_empty() {
    return Some(base);
  }

  let base = config
    .get_string(&format!("branch.{}.vscode-merge-base", &branch))
    .ok()?;

  if !base.is_empty() {
    return Some(base);
  }

  None
}

/// Sets feature-base of a branch
pub fn set_feature_base(config: &mut Config, branch: &str, base: &str) -> CliResult {
  config
    .set_str(&format!("branch.{}.feature-base", &branch), base)
    .map_err(cli_err_fn!(
      Git,
      e,
      "Failed to save base branch to git config: {e}"
    ))?;

  Ok(())
}
