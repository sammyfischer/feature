//! Interactions with persistent data

use anyhow::{Context, Result};
use git2::{Config, ErrorClass, ErrorCode, Repository};

use crate::util::branch_meta::BranchMeta;

/// Gets the feature-base of a branch.
///
/// # Params
/// - `branch_name` - the shortname of the branch (use [BranchMeta::name] if available)
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

/// Gets "feature.user" from git config
pub fn get_feature_user(config: &Config) -> Result<Option<String>> {
  Ok(match config.get_string("feature.user") {
    Ok(it) => Some(it),
    Err(e) if e.class() == ErrorClass::Config && e.code() == ErrorCode::NotFound => None,
    Err(e) => return Err(e.into()),
  })
}

/// Gets "feature.format.graph" from git config
pub fn get_format_graph(config: &Config) -> Result<Option<String>> {
  Ok(match config.get_string("feature.format.graph") {
    Ok(it) => Some(it),
    Err(e) if e.class() == ErrorClass::Config && e.code() == ErrorCode::NotFound => None,
    Err(e) => return Err(e.into()),
  })
}

/// Gets "feature.format.date" from git config
pub fn get_format_date(config: &Config) -> Result<Option<String>> {
  Ok(match config.get_string("feature.format.date") {
    Ok(it) => Some(it),
    Err(e) if e.class() == ErrorClass::Config && e.code() == ErrorCode::NotFound => None,
    Err(e) => return Err(e.into()),
  })
}

/// Gets "feature.format.relative" from git config. Defaults to `false`.
pub fn get_format_relative(config: &Config) -> Result<bool> {
  Ok(match config.get_bool("feature.format.date") {
    Ok(it) => it,
    Err(e) if e.class() == ErrorClass::Config && e.code() == ErrorCode::NotFound => false,
    Err(e) => return Err(e.into()),
  })
}
