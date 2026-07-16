//! Manage the user's config. User config is stored in git config.

use anyhow::{Context, Result, anyhow};
use clap::ValueEnum;
use git2::{Config, ErrorClass, ErrorCode, Repository};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::NotFoundExt;
use crate::core::branch_info::BranchInfo;
use crate::core::project_config::PageWhen;

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
/// In both forms, `config` is an instance of [git2::Config] and `getter` is one
/// of the getter functions defined it.
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

/// Personal config options, stored in git config. This is essentially a handle
/// to a read-only snapshot of the git config.
pub struct UserConfig<'config> {
  repo: &'config Repository,
  config: Config,
}

/// The level of a commit message to show
#[derive(
  Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum CommitMessageLevel {
  None,
  Subject,
  #[default]
  Full,
}

impl<'config> UserConfig<'config> {
  pub fn new(repo: &'config Repository) -> Result<Self> {
    let config = repo.config()?.snapshot()?;
    Ok(Self { repo, config })
  }

  /// Gets a branch's base from its config: `branch.<branch_name>.feature-base`.
  ///
  /// # Param
  /// - `branch_name` - the short name of the branch
  pub fn branch_base(&self, branch_name: &str) -> Result<Option<BranchInfo>> {
    self
      .config
      .get_string(&format!("branch.{}.feature-base", &branch_name))
      .not_found_ok()?
      .map(|refname| BranchInfo::from_refname(self.repo, &refname))
      .transpose()
  }

  /// `feature.project`, default `false`
  pub fn project(&self) -> Result<bool> {
    get_option!(&self.config, get_bool, "feature.project", false)
  }

  /// `feature.user`
  pub fn user(&self) -> Result<Option<String>> {
    get_option!(&self.config, get_string, "feature.user")
  }

  /// `feature.autofetch`, default `true`
  pub fn autofetch(&self) -> Result<bool> {
    get_option!(&self.config, get_bool, "feature.autofetch", true)
  }

  /// `feature.showAuthorship`, default `true`
  pub fn show_authorship(&self) -> Result<bool> {
    get_option!(&self.config, get_bool, "feature.showAuthorship", true)
  }

  /// `feature.showProjects`, default `true`
  pub fn show_projects(&self) -> Result<bool> {
    get_option!(&self.config, get_bool, "feature.showProjects", true)
  }

  /// `feature.showModules`, default `true`
  pub fn show_modules(&self) -> Result<bool> {
    get_option!(&self.config, get_bool, "feature.showModules", true)
  }

  // FORMAT

  /// `feature.format.date`, default `"%b %d, %Y at %I:%M %p"`
  pub fn format_date(&self) -> Result<String> {
    get_option!(
      &self.config,
      get_string,
      "feature.format.date",
      "%b %d, %Y at %I:%M %p".to_string()
    )
  }

  /// `feature.format.relative`, default `false`
  pub fn format_relative(&self) -> Result<bool> {
    get_option!(&self.config, get_bool, "feature.format.relative", false)
  }

  // ADVICE

  /// `advice.statusHints`, default `false`
  pub fn advice_status(&self) -> Result<bool> {
    get_option!(&self.config, get_bool, "advice.statusHints", false)
  }

  /// `advice.resolveConflict`, default `true`.
  pub fn advice_conflict(&self) -> Result<bool> {
    get_option!(&self.config, get_bool, "advice.resolveConflict", true)
  }

  // END

  /// `feature.end.remote`, default `false`
  pub fn end_remote(&self) -> Result<bool> {
    get_option!(&self.config, get_bool, "feature.end.remote", false)
  }

  // SHOW

  /// Gets `feature.show.message`. Defaults to
  /// [DisplayCommitMessageLevel::default()]
  pub fn show_message(&self) -> Result<CommitMessageLevel> {
    let value =
      (get_option!(&self.config, get_str, "feature.show.message") as Result<Option<&str>>)?;
    Ok(match value {
      Some(value) => CommitMessageLevel::from_str(value, true).map_err(|e| anyhow!(e))?,
      None => CommitMessageLevel::default(),
    })
  }

  /// `feature.show.summary`, default `true`
  pub fn show_summary(&self) -> Result<bool> {
    get_option!(&self.config, get_bool, "feature.show.summary", true)
  }

  /// `feature.show.patch`, default `false`
  pub fn show_patch(&self) -> Result<bool> {
    get_option!(&self.config, get_bool, "feature.show.patch", false)
  }

  /// `feature.show.paging`, default [PageWhen::default]
  pub fn show_paging(&self) -> Result<PageWhen> {
    let value =
      (get_option!(&self.config, get_str, "feature.show.paging") as Result<Option<&str>>)?;
    Ok(match value {
      Some(value) => PageWhen::from_str(value, true).map_err(|e| anyhow!(e))?,
      None => PageWhen::default(),
    })
  }

  // STATUS

  /// `status.showUntrackedFiles`, default `true`
  pub fn status_untracked(&self) -> Result<bool> {
    get_option!(&self.config, get_bool, "status.showUntrackedFiles", true)
  }

  // SYNC

  /// `feature.sync.prune`, default `true`
  pub fn sync_prune(&self) -> Result<bool> {
    get_option!(&self.config, get_bool, "feature.sync.prune", true)
  }

  /// `feature.sync.projects`, default `true`
  pub fn sync_projects(&self) -> Result<bool> {
    get_option!(&self.config, get_bool, "feature.sync.projects", true)
  }

  /// `feature.sync.modules`, default `true`
  pub fn sync_modules(&self) -> Result<bool> {
    get_option!(&self.config, get_bool, "feature.sync.modules", true)
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
