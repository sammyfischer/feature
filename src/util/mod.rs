//! Helper functions that may be found useful in many places

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use anyhow::{Context, Result, anyhow};
use console::style;
use git2::{
  Commit,
  Cred,
  CredentialType,
  ErrorCode,
  Oid,
  PackBuilderStage,
  RemoteCallbacks,
  Repository,
  Signature,
  Tag,
};
use indicatif::{BinaryBytes, HumanCount, MultiProgress, ProgressBar, ProgressStyle};

use crate::util::branch::find_branch_at_commit;
use crate::util::display::{display_hash, trim_hash};
use crate::util::string::{ToStrLossy, ToStrLossyOwned, TrimPrefix};
use crate::util::term::{PROGRESS_CHARS, TICK_STRINGS};
use crate::{await_child, git};

pub mod advice;
pub mod branch;
pub mod branch_meta;
pub mod diff;
pub mod display;
pub mod string;
pub mod tag;
pub mod term;

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

  for name in tags.iter().flatten() {
    let name = name.context("Tag names must be valid utf-8")?;
    let reference = repo.find_reference(&format!("refs/tags/{}", name))?;
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

/// Buffers that contain push info
pub struct PushOutput {
  /// Each branch that was updated in the local repository
  pub updates: Rc<RefCell<String>>,

  /// Each branch that failed to push
  pub rejections: Rc<RefCell<String>>,

  /// Arbitrary server reponse
  pub server: Rc<RefCell<String>>,
}

impl PushOutput {
  pub fn new() -> Self {
    PushOutput {
      updates: Rc::new(RefCell::new(String::new())),
      rejections: Rc::new(RefCell::new(String::new())),
      server: Rc::new(RefCell::new(String::new())),
    }
  }

  /// Prints output to stdout
  pub fn print(self) {
    if let Some(updates) = Rc::into_inner(self.updates) {
      print!("{}", updates.into_inner());
    }
    if let Some(rejections) = Rc::into_inner(self.rejections) {
      print!("{}", rejections.into_inner());
    }
    if let Some(response) = Rc::into_inner(self.server) {
      print!("{}", response.into_inner());
    }
  }
}

/// Gets fully configured push callbacks
pub fn get_push_callbacks<'cbs, 'repo: 'cbs>(
  repo: &'repo Repository,
  bufs: &'cbs mut PushOutput,
) -> Result<RemoteCallbacks<'cbs>> {
  use std::fmt::Write;

  let multi = MultiProgress::new();
  let mut cbs = RemoteCallbacks::new();

  cbs.credentials(get_credentials_cb());

  let pack_progress = multi.add(ProgressBar::new_spinner().with_style(
    ProgressStyle::with_template("{spinner:.cyan} {elapsed} {msg}")?.tick_strings(&TICK_STRINGS),
  ));

  cbs.pack_progress(move |stage, current, total| {
    pack_progress.set_message(format!(
      "{}: {}/{}",
      match stage {
        PackBuilderStage::AddingObjects => "Counting objects",
        PackBuilderStage::Deltafication => "Compressing objects",
      },
      current,
      total
    ));

    // Finish the spinner once packing completes
    if current == total && total > 0 {
      pack_progress.finish_with_message(format!("Packed {} objects", HumanCount(total as u64)));
    }
  });

  let transfer_progress = multi.add(
    ProgressBar::new(0).with_style(
      ProgressStyle::with_template("{spinner:.cyan} {elapsed} [{bar:40.cyan}] {msg}")?
        .progress_chars(PROGRESS_CHARS)
        .tick_strings(&TICK_STRINGS),
    ),
  );

  cbs.push_transfer_progress(move |current, total, bytes| {
    if transfer_progress.length().is_none() || transfer_progress.length() == Some(0) {
      transfer_progress.set_length(total as u64);
    }

    transfer_progress.set_position(current as u64);

    if current != total {
      transfer_progress.set_message(format!("Transferring {}/{} objects", current, total));
    } else {
      transfer_progress.finish_with_message(format!(
        "Transferred {} objects ({})",
        HumanCount(total as u64),
        BinaryBytes(bytes as u64)
      ));
    }
  });

  // called on each remote tracking branch that's updated
  let update_buf = bufs.updates.clone();
  cbs.update_tips(move |name: &str, old_id: Oid, new_id: Oid| -> bool {
    if old_id == new_id {
      return true;
    }

    let name = name.trim_prefix_opt("refs/remotes/");
    let zero = Oid::ZERO_SHA1;

    match (old_id, new_id) {
      (old, new) if old == zero && new != zero => {
        if let Ok(new_commit) = repo.find_commit(new_id)
          && let Ok(hash) = display_hash(&new_commit)
        {
          let _ = writeln!(
            update_buf.borrow_mut(),
            "{} {} {}",
            style("Created").green(),
            name,
            hash
          );
        };
      }

      (old, new) if new == zero && old != zero => {
        if let Ok(old_commit) = repo.find_commit(old_id)
          && let Ok(hash) = trim_hash(&old_commit)
        {
          let _ = writeln!(
            update_buf.borrow_mut(),
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
          let _ = writeln!(
            update_buf.borrow_mut(),
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
  });

  // print error if push fails
  let rejection_buf = bufs.rejections.clone();
  cbs.push_update_reference(move |refname, status| {
    // a status of Some means push was rejected
    if let Some(msg) = status {
      let _ = writeln!(
        rejection_buf.borrow_mut(),
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

  // this is arbitrary text sent by the server. on github/gitlab, this usually
  // contains info on how to create a pull request for newly pushed branches
  let response_buf = bufs.server.clone();
  cbs.sideband_progress(move |bytes| {
    let _ = write!(response_buf.borrow_mut(), "{}", bytes.to_str_lossy());
    true
  });

  Ok(cbs)
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
