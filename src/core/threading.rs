use std::path::PathBuf;

use anyhow::Result;
use git2::Repository;

/// A handle used to open repos from a thread context. While this satisfies
/// compiler, it's only safe to read from the repo.
pub struct ThreadedRepoHandle {
  git_dir: PathBuf,
  work_dir: Option<PathBuf>,
}

impl ThreadedRepoHandle {
  /// Get a new handle from a repo
  pub fn from(repo: &Repository) -> Self {
    Self {
      git_dir: repo.path().to_owned(),
      work_dir: repo.workdir().map(ToOwned::to_owned),
    }
  }

  /// Open the repo that this handle wraps
  pub fn open(&self) -> Result<Repository> {
    match &self.work_dir.as_deref() {
      Some(work_dir) => {
        let repo = Repository::open_bare(&self.git_dir)?;
        repo.set_workdir(work_dir, false)?;
        Ok(repo)
      }
      None => Ok(Repository::open(&self.git_dir)?),
    }
  }
}
