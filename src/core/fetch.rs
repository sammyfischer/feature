use anyhow::{Context, Result, anyhow};
use git2::{
  AutotagOption,
  BranchType,
  Cred,
  CredentialType,
  FetchOptions,
  FetchPrune,
  RemoteCallbacks,
  Repository,
};

use crate::core::branch_info::BranchInfo;

/// Gets the callback used in fetches/pushes to handle authentication
pub fn get_credentials_cb()
-> impl FnMut(&str, Option<&str>, CredentialType) -> core::result::Result<Cred, git2::Error> {
  let mut tried_agent = false;
  move |url: &str,
        username_from_url: Option<&str>,
        allowed_types: CredentialType|
        -> core::result::Result<Cred, git2::Error> {
    if allowed_types.contains(CredentialType::USERNAME) {
      return Cred::username(username_from_url.unwrap_or("git"));
    }

    if allowed_types.contains(CredentialType::SSH_KEY) && !tried_agent {
      tried_agent = true;
      return Cred::ssh_key_from_agent(username_from_url.unwrap_or("git"));
    }

    if allowed_types.contains(CredentialType::USER_PASS_PLAINTEXT) {
      if let Ok(cred) =
        Cred::credential_helper(&git2::Config::open_default()?, url, username_from_url)
      {
        return Ok(cred);
      }

      // fallback to git token env var
      let token = std::env::var("GIT_TOKEN").map_err(|_| {
        git2::Error::from_str(
          "Failed to find credentials. Try setting the GIT_TOKEN environment variable",
        )
      })?;

      return Cred::userpass_plaintext(username_from_url.unwrap_or("git"), &token);
    }

    if allowed_types.contains(CredentialType::DEFAULT) {
      return Cred::default();
    }

    Err(git2::Error::from_str(&format!(
      "No supported credential type for {}",
      url
    )))
  }
}

/// Fetches all remote branches
pub fn fetch_all(repo: &Repository) -> Result<()> {
  let remotes = repo.remotes()?;
  let mut results: Vec<Result<()>> = Vec::with_capacity(remotes.len());

  for name in remotes.iter().flatten() {
    let name = name.context("Remote names must be valid utf-8")?;
    let mut remote = repo.find_remote(name)?;
    let mut cbs = RemoteCallbacks::new();
    cbs.credentials(get_credentials_cb());

    let mut opts = FetchOptions::new();
    opts.remote_callbacks(cbs);
    opts.prune(FetchPrune::On);
    opts.download_tags(AutotagOption::All);

    results.push(
      remote
        .fetch(
          &[format!("+refs/heads/*:refs/remotes/{}/*", name)],
          Some(&mut opts),
          None,
        )
        .map_err(|e| anyhow!("{}", e)),
    );
  }

  for result in results {
    if let Err(e) = result {
      eprintln!("{}", e);
    }
  }

  Ok(())
}

/// Fetch a single branch. `branch` must be a remote branch.
///
/// # Panics
/// If `branch` is not a remote branch
pub fn fetch_upstream_branch(repo: &Repository, branch: &BranchInfo) -> Result<()> {
  assert!(
    branch.ty() == BranchType::Remote,
    "Cannot fetch {}: not a remote branch",
    branch.refname()
  );

  let (shortname, remote_name) = branch.split_name_and_remote()?;
  let mut remote = repo.find_remote(&remote_name.unwrap_or_else(|| {
    panic!(
      "Remote should exist on upstream branch: {}",
      branch.refname()
    )
  }))?;

  let refspec = format!("+refs/heads/{}:{}", shortname, branch.refname());

  let mut opts = FetchOptions::new();
  let mut cbs = RemoteCallbacks::new();
  cbs.credentials(get_credentials_cb());
  opts.remote_callbacks(cbs);

  remote.fetch(&[&refspec], Some(&mut opts), None)?;
  Ok(())
}
