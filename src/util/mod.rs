//! Helper functions that may be found useful in many places

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use console::style;
use git2::{Commit, Cred, CredentialType, ErrorCode, Oid, Repository, Signature, Tag};

use crate::util::branch::commit_to_branch;
use crate::util::display::{display_hash, trim_hash};
use crate::util::lossy::ToStrLossyOwned;

pub mod advice;
pub mod branch;
pub mod branch_meta;
pub mod diff;
pub mod display;
pub mod lossy;
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
pub fn resolve_commit_name(repo: &Repository, commit: &Commit) -> Result<String> {
  if let Some(branch) = commit_to_branch(repo, &commit.id())? {
    return Ok(branch.name_bytes()?.to_str_lossy_owned());
  }

  if let Some(tag) = commit_to_tag(repo, &commit.id())? {
    return Ok(tag.name_bytes().to_str_lossy_owned());
  }

  trim_hash(commit)
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

pub fn credentials_cb(
  url: &str,
  username_from_url: Option<&str>,
  allowed_types: CredentialType,
) -> Result<Cred, git2::Error> {
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
}

pub fn get_update_tips_cb(repo: &Repository) -> impl Fn(&str, Oid, Oid) -> bool {
  |name: &str, old_id: Oid, new_id: Oid| -> bool {
    if old_id == new_id {
      return true;
    }

    let name = name.trim_prefix("refs/remotes/");
    let zero = Oid::zero();

    if old_id == zero {
      let Ok(new_commit) = repo.find_commit(new_id) else {
        return false;
      };
      let Ok(hash) = display_hash(&new_commit) else {
        return false;
      };

      println!("{} {} {}", style("Created").green(), name, hash);
    } else if new_id == zero {
      let Ok(old_commit) = repo.find_commit(old_id) else {
        return false;
      };
      let Ok(hash) = trim_hash(&old_commit) else {
        return false;
      };

      println!(
        "{} {} {}",
        style("Deleted").red(),
        name,
        style(&format!("(was {})", hash)).dim()
      );
    } else {
      let Ok(new_commit) = repo.find_commit(new_id) else {
        return false;
      };
      let Ok(new_hash) = display_hash(&new_commit) else {
        return false;
      };

      let Ok(old_commit) = repo.find_commit(old_id) else {
        return false;
      };
      let Ok(old_hash) = display_hash(&old_commit) else {
        return false;
      };

      println!(
        "{} {}: {} -> {}",
        style("Updated").green(),
        name,
        old_hash,
        new_hash
      );
    }
    true
  }
}
