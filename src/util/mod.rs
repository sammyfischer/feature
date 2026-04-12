//! Helper functions that may be found useful in many places

use anyhow::{Context, Result, anyhow};
use git2::{Commit, Cred, CredentialType, ErrorCode, RemoteCallbacks, Repository, Signature};

pub mod advice;
pub mod branch;
pub mod diff;
pub mod display;
pub mod term;

pub fn get_current_commit<'repo>(repo: &'repo Repository) -> Result<Option<Commit<'repo>>> {
  let head = match repo.head() {
    Ok(it) => it,
    Err(e) if e.code() == ErrorCode::UnbornBranch => return Ok(None),
    Err(e) => return Err(e.into()),
  };

  let commit = head
    .peel_to_commit()
    .context("Failed to get commit pointed to by HEAD")?;

  Ok(Some(commit))
}

pub fn get_signature<'repo>(repo: &'repo Repository) -> Result<Option<Signature<'repo>>> {
  match repo.signature() {
    Ok(it) => Ok(Some(it)),
    Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
    Err(e) => Err(anyhow!(e).context("Failed to get default signature")),
  }
}

/// Gets remote callbacks to use for remote operations with git2
pub fn get_remote_callbacks<'repo>() -> RemoteCallbacks<'repo> {
  let mut callbacks = RemoteCallbacks::new();

  callbacks.credentials(|url, username_from_url, allowed_types| {
    if allowed_types.contains(CredentialType::SSH_KEY) {
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
  });

  callbacks
}
