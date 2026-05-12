//! Interactions with persistent data

use anyhow::{Context, Result, anyhow};
use clap::ValueEnum;
use git2::{Config, ErrorClass, ErrorCode, Repository};

use crate::{
  config::PageWhen,
  util::{branch_meta::BranchMeta, display::DisplayCommitMessageLevel},
};

/// Generates the function to get a variable from git config.
///
/// In the first form, the arguments are:
/// ```
/// get_option!(config, getter, key)
/// ```
///
/// The returned value will be an `Option` wrapping the possible value.
///
/// In the second form, the arguments are:
/// ```
/// get_option!(config, getter, key, default)
/// ```
/// The returned value won't be an option, and if not found will use `default`.
///
/// In both forms, `config` is an instance of [git2::Config] and `getter` is one of the getter functions defined it.
///
/// `key` is always a string.
macro_rules! get_option {
  ($config:expr, $getter:ident, $key:literal) => {
    Ok(match $config.$getter($key) {
      Ok(it) => Some(it),
      Err(e) if e.class() == ErrorClass::Config && e.code() == ErrorCode::NotFound => None,
      Err(e) => return Err(e.into()),
    })
  };

  ($config:expr, $getter:ident, $key:literal, $default:expr) => {
    Ok(match $config.$getter($key) {
      Ok(it) => it,
      Err(e) if e.class() == ErrorClass::Config && e.code() == ErrorCode::NotFound => $default,
      Err(e) => return Err(e.into()),
    })
  };
}

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

/// Gets `feature.user`
pub fn get_feature_user(config: &Config) -> Result<Option<String>> {
  get_option!(config, get_string, "feature.user")
}

/// Gets `feature.end.remote`. Defaults to `false`.
pub fn get_end_remote(config: &Config) -> Result<bool> {
  get_option!(config, get_bool, "feature.end.remote", false)
}

/// Gets `feature.sync.prune`. Defaults to `true`.
pub fn get_sync_prune(config: &Config) -> Result<bool> {
  get_option!(config, get_bool, "feature.sync.prune", true)
}

/// Gets `status.showUntrackedFiles`. Defaults to `true`.
pub fn get_status_untracked(config: &Config) -> Result<bool> {
  get_option!(config, get_bool, "status.showUntrackedFiles", true)
}

/// Gets `feature.status.showModules`. Defaults to `true`.
pub fn get_status_modules(config: &Config) -> Result<bool> {
  get_option!(config, get_bool, "feature.status.showModules", true)
}

/// Gets `feature.format.graph`
pub fn get_format_graph(config: &Config) -> Result<Option<String>> {
  get_option!(config, get_string, "feature.format.graph")
}

/// Gets `feature.format.date`
pub fn get_format_date(config: &Config) -> Result<Option<String>> {
  get_option!(config, get_string, "feature.format.date")
}

/// Gets `feature.format.relative`. Defaults to `false`.
pub fn get_format_relative(config: &Config) -> Result<bool> {
  get_option!(config, get_bool, "feature.format.relative", false)
}

/// Gets `advice.statusHints`. Defaults to `false`.
pub fn get_advice_status(config: &Config) -> Result<bool> {
  get_option!(config, get_bool, "advice.statusHints", false)
}

/// Gets `advice.resolveConflict`. Defaults to `true`.
pub fn get_advice_conflict(config: &Config) -> Result<bool> {
  get_option!(config, get_bool, "advice.resolveConflict", true)
}

/// Gets `feature.list.hash`. Defautls to `true`.
pub fn get_list_hash(config: &Config) -> Result<bool> {
  get_option!(config, get_bool, "feature.list.hash", true)
}

/// Gets `feature.list.upstream`. Defautls to `true`.
pub fn get_list_upstream(config: &Config) -> Result<bool> {
  get_option!(config, get_bool, "feature.list.upstream", true)
}

/// Gets `feature.list.base`. Defautls to `true`.
pub fn get_list_base(config: &Config) -> Result<bool> {
  get_option!(config, get_bool, "feature.list.base", true)
}

/// Gets `feature.show.message`. Defaults to [DisplayCommitMessageLevel::default()].
pub fn get_show_message(config: &Config) -> Result<DisplayCommitMessageLevel> {
  let value = (get_option!(config, get_str, "feature.show.message") as Result<Option<&str>>)?;
  Ok(match value {
    Some(value) => DisplayCommitMessageLevel::from_str(value, true).map_err(|e| anyhow!(e))?,
    None => DisplayCommitMessageLevel::default(),
  })
}

/// Gets `feature.show.summary`. Defaults to `true`.
pub fn get_show_summary(config: &Config) -> Result<bool> {
  get_option!(config, get_bool, "feature.show.summary", true)
}

/// Gets `feature.show.patch`. Defaults to `false`.
pub fn get_show_patch(config: &Config) -> Result<bool> {
  get_option!(config, get_bool, "feature.show.patch", false)
}

/// Gets `feature.show.paging`. Defaults to [PageWhen::default()].
pub fn get_show_paging(config: &Config) -> Result<PageWhen> {
  let value = (get_option!(config, get_str, "feature.show.paging") as Result<Option<&str>>)?;
  Ok(match value {
    Some(value) => PageWhen::from_str(value, true).map_err(|e| anyhow!(e))?,
    None => PageWhen::default(),
  })
}
