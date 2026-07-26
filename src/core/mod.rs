//! Core functionality of feature. These implementations should be
//! frontend-agnostic.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use git2::{ErrorClass, ErrorCode, Object, Repository};

use crate::core::string::ToStrLossyOwned;
use crate::{await_child, git};

pub mod branch;
pub mod branch_info;
pub mod commit;
pub mod diff;
pub mod fetch;
pub mod project;
pub mod project_config;
pub mod push;
pub mod semver;
pub mod status;
pub mod string;
pub mod user_config;
pub mod wip;

/// An extension trait to map non-existence errors into options, while
/// maintaining any other errors.
pub trait NotFoundExt<T> {
  /// Converts [NotFound] errors to `Ok(None)`. Wraps all other errors in
  /// [anyhow::Error].
  ///
  /// [NotFound]: git2::ErrorCode::NotFound
  fn not_found_ok(self) -> Result<Option<T>, anyhow::Error>;

  /// When used on one of the `Repository::open*` functions, returns `Ok(None)`
  /// when the repo is not found.
  ///
  /// [NotFound]: git2::ErrorCode::NotFound
  fn repo_not_found_ok(self) -> Result<Option<T>, anyhow::Error>;
}

impl<T> NotFoundExt<T> for core::result::Result<T, git2::Error> {
  fn not_found_ok(self) -> Result<Option<T>, anyhow::Error> {
    match self {
      Ok(it) => Ok(Some(it)),
      Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
      Err(e) => Err(anyhow!(e)),
    }
  }

  fn repo_not_found_ok(self) -> Result<Option<T>, anyhow::Error> {
    match self {
      Ok(it) => Ok(Some(it)),
      Err(e)
      // class == Os         => workdir doesn't exist
      // class == Repository => workdir exists, but no .git dir
        if (e.class() == ErrorClass::Os || e.class() == ErrorClass::Repository)
          && e.code() == ErrorCode::NotFound =>
      {
        Ok(None)
      }
      Err(e) => Err(anyhow!(e)),
    }
  }
}

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
