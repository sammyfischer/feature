//! Helper functions that may be found useful in many places

use anyhow::{Context, Result, anyhow};
use console::style;
use git2::{
  Commit, Cred, CredentialType, ErrorCode, Oid, RemoteCallbacks, Repository, Signature, Tag,
};

use crate::util::branch::find_branch_at_commit;
use crate::util::display::{display_hash, trim_hash};
use crate::util::string::{ToStrLossy, ToStrLossyOwned, TrimPrefix};
use crate::{await_child, git};

pub mod advice;
pub mod branch;
pub mod branch_meta;
pub mod diff;
pub mod display;
pub mod string;
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

/// Finds a tag that points to the given commit
pub fn find_tag_at_commit<'repo>(
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
/// 3. Getting the abbreviated commit hash
pub fn resolve_commit_name(repo: &Repository, commit: &Commit) -> Result<String> {
  if let Some(branch) = find_branch_at_commit(repo, &commit.id())? {
    return Ok(branch.name_bytes()?.to_str_lossy_owned());
  }

  if let Some(tag) = find_tag_at_commit(repo, &commit.id())? {
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

/// The callback used in fetches/pushes to handle authentication
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

/// Gets the callback used in pushes when a reference is updated
pub fn get_update_tips_cb(repo: &Repository) -> impl Fn(&str, Oid, Oid) -> bool {
  |name: &str, old_id: Oid, new_id: Oid| -> bool {
    if old_id == new_id {
      return true;
    }

    let name = name.trim_prefix_opt("refs/remotes/");
    let zero = Oid::zero();

    match (old_id, new_id) {
      (old, new) if old == zero && new != zero => {
        if let Ok(new_commit) = repo.find_commit(new_id)
          && let Ok(hash) = display_hash(&new_commit)
        {
          println!("{} {} {}", style("Created").green(), name, hash);
        };
      }

      (old, new) if new == zero && old != zero => {
        if let Ok(old_commit) = repo.find_commit(old_id)
          && let Ok(hash) = trim_hash(&old_commit)
        {
          println!(
            "{} {} {}",
            style("Deleted").red(),
            name,
            style(&format!("(was {})", hash)).dim()
          );
        };
      }

      (old, new) => {
        if let Ok(new_commit) = repo.find_commit(new)
          && let Ok(new_hash) = display_hash(&new_commit)
          && let Ok(old_commit) = repo.find_commit(old)
          && let Ok(old_hash) = display_hash(&old_commit)
        {
          println!(
            "{} {} {} -> {}",
            style("Updated").green(),
            name,
            old_hash,
            new_hash
          );
        };
      }
    }

    true
  }
}

/// Gets fully configured push callbacks
pub fn get_push_callbacks<'cbs>(repo: &'cbs Repository) -> RemoteCallbacks<'cbs> {
  let mut cbs = RemoteCallbacks::new();

  cbs.credentials(credentials_cb);

  // called on each remote tracking branch that's updated
  cbs.update_tips(get_update_tips_cb(repo));

  // print error if push fails
  cbs.push_update_reference(|refname, status| {
    // a status of Some means push was rejected
    if let Some(msg) = status {
      eprintln!(
        "{} to {} {}: {}",
        style("Push").red(),
        refname,
        style("failed").red(),
        msg
      );
      return Err(git2::Error::from_str(msg));
    }
    Ok(())
  });

  // this is arbitrary text sent by the server. on github/gitlab, this usually contains info on
  // how to create a pull request for newly pushed branches
  cbs.sideband_progress(|bytes| {
    print!("{}", bytes.to_str_lossy());
    true
  });

  cbs
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
