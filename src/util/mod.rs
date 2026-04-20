//! Helper functions that may be found useful in many places

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use git2::{
  Commit,
  Cred,
  CredentialType,
  ErrorCode,
  Oid,
  RemoteCallbacks,
  Repository,
  Signature,
  Tag,
};

use crate::lossy;
use crate::util::branch::commit_to_branch;
use crate::util::display::trim_hash;

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

pub fn commit_to_tag<'repo>(
  repo: &'repo Repository,
  commit_id: &'repo Oid,
) -> Result<Option<Tag<'repo>>> {
  let tags = repo.tag_names(None)?;

  for tag_name in tags.iter().flatten() {
    let reference = repo.find_reference(&format!("refs/tags/{}", tag_name))?;
    let tag = reference.peel_to_tag()?;
    let tag_commit = reference.peel_to_commit()?;

    if commit_id == &tag_commit.id() {
      return Ok(Some(tag));
    }
  }

  Ok(None)
}

/// Finds a good user-friendly display name for a commit. Tries:
///
/// 1. To find a branch matching the commit, yielding the short branch name
/// 2. To find a tag matching the commit, yielding the short tag name
///
/// If all else fails, returns the trimmed commit hash.
pub fn resolve_commit_name(repo: &Repository, commit_id: &Oid) -> Result<String> {
  if let Some(branch) = commit_to_branch(repo, commit_id)? {
    return Ok(lossy!(branch.name_bytes()?).to_string());
  }

  if let Some(tag) = commit_to_tag(repo, commit_id)? {
    return Ok(lossy!(tag.name_bytes()).to_string());
  }

  Ok(trim_hash(commit_id))
}

pub fn get_signature<'repo>(repo: &'repo Repository) -> Result<Option<Signature<'repo>>> {
  match repo.signature() {
    Ok(it) => Ok(Some(it)),
    Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
    Err(e) => Err(anyhow!(e).context("Failed to get default signature")),
  }
}

/// Reads an entire formatted commit msg file and removes the comments
pub fn read_commit_msg(path: &Path) -> Result<String> {
  let text = fs::read_to_string(path)?;
  let mut real_lines = Vec::new();

  for line in text.lines() {
    if !line.starts_with('#') {
      real_lines.push(line);
    }
  }

  Ok(real_lines.join("\n"))
}

/// Gets remote callbacks with configured credential handling
/// # Lifetimes
/// - `cbs` - the lifetime of each callback function
pub fn get_remote_callbacks<'cbs>() -> RemoteCallbacks<'cbs> {
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
