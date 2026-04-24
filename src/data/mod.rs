//! Interactions with persistent data

use anyhow::{Context, Result};
use git2::{Config, ErrorCode, Repository};

use crate::util::branch_meta::BranchMeta;

/// Gets the feature-base of a branch
pub fn get_feature_base(repo: &Repository, branch_name: &str) -> Result<Option<BranchMeta>> {
  match repo
    .config()?
    .get_string(&format!("branch.{}.feature-base", &branch_name))
  {
    Ok(it) => Ok(Some(
      BranchMeta::from_refname(repo, &it).context("Failed to parse base branch name")?,
    )),
    Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
    Err(e) => Err(e.into()),
  }
}

/// Sets feature-base of a branch
/// # Params
/// - `branch_name` - the shorthand name of the branch
/// - `base_refname` - the full refname of the base branch
pub fn set_feature_base(config: &mut Config, branch_name: &str, base_refname: &str) -> Result<()> {
  config
    .set_str(
      &format!("branch.{}.feature-base", &branch_name),
      base_refname,
    )
    .with_context(|| {
      format!(
        "Failed to set branch '{}' to use base '{}' in git config",
        branch_name, base_refname
      )
    })?;

  Ok(())
}
