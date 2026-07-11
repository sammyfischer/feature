//! Core functionality of feature. These implementations should be
//! frontend-agnostic.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use git2::{ErrorCode, Object, Repository, Signature};

use crate::core::string::ToStrLossyOwned;
use crate::{await_child, git};

pub mod advice;
pub mod branch;
pub mod branch_info;
pub mod commit;
pub mod diff;
pub mod display;
pub mod fetch;
pub mod project_config;
pub mod push;
pub mod status;
pub mod string;
pub mod tag;
pub mod term;
pub mod user_config;
pub mod wip;

/// Opens a repo given a `.git` dir. If the workdir is `None`, it's assumed to
/// be the parent of repo_dir. If `Some`, the repo is assumed to be bare.
///
/// This is a useful way to open the same repo in multiple threads.
/// ```
/// // Get dirs from existing repo. These must be owned, since borrowing from
/// // the repo means sharing partial references to the repo between threads.
/// let repo_dir = repo.path().to_owned();
/// let work_dir = repo.workdir().to_owned();
///
/// // in a thread context, open the repo
/// let repo = open_repo_from_dirs(&repo_dir, work_dir.as_deref())?;
/// ```
///
/// In general this is only safe if you're only reading from the repo in each
/// thread.
pub fn open_repo_from_dirs(repo_dir: &Path, work_dir: Option<&Path>) -> Result<Repository> {
  match &work_dir {
    Some(work_dir) => {
      let repo = Repository::open_bare(repo_dir)?;
      repo.set_workdir(work_dir, false)?;
      Ok(repo)
    }
    None => Ok(Repository::open(repo_dir)?),
  }
}

/// Gets the short id of the given object
pub fn trim_hash(obj: &Object) -> Result<String> {
  Ok(obj.short_id()?.to_str_lossy_owned())
}

pub fn get_signature<'repo>(repo: &'repo Repository) -> Result<Option<Signature<'repo>>> {
  match repo.signature() {
    Ok(it) => Ok(Some(it)),
    Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
    Err(e) => Err(anyhow!(e).context("Failed to get default signature")),
  }
}

/// Deletes an entire section from git config
pub fn delete_config_section(key: &str) -> Result<()> {
  match git!("config", "--remove-section", &key).spawn() {
    Ok(mut cmd) => await_child!(cmd, "Git"),
    Err(e) => Err(e.into()),
  }
  .with_context(|| {
    format!(
      "Failed to delete branch config. Run \"git config --remove-section {}\" to remove it.",
      key
    )
  })?;
  Ok(())
}
