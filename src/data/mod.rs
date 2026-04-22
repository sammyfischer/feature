//! Interactions with persistent data

use anyhow::{Context, Result};
use git2::{Branch, Config, ErrorCode, Repository};

use crate::util::branch::name_to_branch;

/// Gets the feature-base of a branch
pub fn get_feature_base<'repo>(
  repo: &'repo Repository,
  branch_name: &str,
) -> Result<Option<Branch<'repo>>> {
  match repo
    .config()?
    .get_string(&format!("branch.{}.feature-base", &branch_name))
  {
    Ok(it) => Ok(name_to_branch(
      repo,
      it.trim_prefix("refs/remotes/").trim_prefix("refs/heads/"),
    )?),
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
