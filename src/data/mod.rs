//! Interactions with persistent data

use anyhow::{Context, Result};
use git2::{Config, Repository};

/// Gets the git config and converts errors into `CliError`s
pub fn git_config(repo: &Repository) -> Result<Config> {
  let config = repo
    .config()
    .context("Failed to get this repository's git config")?;
  Ok(config)
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

/// Gets feature-base and strips the refs/remotes/ prefix
pub fn get_short_feature_base(config: &Config, branch: &str) -> Option<String> {
  let full_name = get_feature_base(config, branch)?;
  full_name
    .strip_prefix("refs/remotes/")
    .map(|it| it.to_string())
}

/// Sets feature-base of a branch
pub fn set_feature_base(config: &mut Config, branch: &str, base: &str) -> Result<()> {
  config
    .set_str(&format!("branch.{}.feature-base", &branch), base)
    .with_context(|| {
      format!(
        "Failed to set branch '{}' to use base '{}' in git config",
        branch, base
      )
    })?;

  Ok(())
}
