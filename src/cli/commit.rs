//! Commit subcommand

use std::env::{self, VarError};
use std::fs;
use std::io::Write;
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use clap::ValueHint;
use console::{strip_ansi_codes, style};
use git2::{Commit, Diff, DiffOptions, ErrorCode, Reference, Repository, Tree};

use crate::App;
use crate::config::Config;
use crate::util::advice::NO_SIGNATURE_MSG;
use crate::util::branch::{
  get_current_branch_name, get_head, get_merge_head, get_pick_head, get_revert_head,
};
use crate::util::diff::{DiffSummary, has_index_changes};
use crate::util::display::{
  DisplayCommitMessageLevel, DisplayCommitOptions, DisplayTimeOptions, display_commit, display_hash,
};
use crate::util::lossy::{ToStrLossy, ToStrLossyOwned};
use crate::util::term::get_user_confirmation;
use crate::util::{get_signature, resolve_commit_name};

const AMEND_LONG_HELP: &str = r"Amend the previous commit. Remaining args overwrite the previous commit message.
If no remaining args are specified, the previous commit message is used.";

const CONFIRM_DURING_PICK: &str = r#"
There is currently a cherry-pick active. Cherry-picks are finished by resolving
the conflicts and running "git cherry-pick --continue", rather than committing.

Do you want to commit anyway?"#;

const CONFIRM_DURING_REVERT: &str = r#"
There is currently a revert active. Reverts are finished by resolving the
conflicts and running "git revert --continue", rather than committing.

Do you want to commit anyway?"#;

#[derive(clap::Args, Clone, Debug)]
#[command(
  about = "Commit staged changes",
  disable_help_flag = true,
  disable_help_subcommand = true
)]
pub struct Args {
  /// Where to apply the commit
  #[arg(long, value_name = "BRANCH", value_hint = ValueHint::Other)]
  to: Option<String>,

  /// Invoke the commit message editor
  #[arg(short, long)]
  edit: bool,

  /// Amend the previous commit
  #[arg(long, long_help = AMEND_LONG_HELP)]
  amend: bool,

  /// Change the commit author to yourself
  #[arg(long, requires = "amend")]
  reset_author: bool,

  /// Bypass precommit hooks
  #[arg(long, value_name = "BYPASS")]
  no_verify: bool,

  /// Words to join together as commit message
  #[arg(trailing_var_arg = true, allow_hyphen_values = true, value_hint = ValueHint::Other)]
  words: Vec<String>,
}

struct CommitTarget<'repo> {
  commit: Commit<'repo>,

  /// Something user-friendly to print (ideally branch name, maybe tag or short hash)
  display_name: String,

  /// The ref to update. Will be None if we're not committing to a branch
  refname: Option<String>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    // if there's a pick active and the user has pick advice enabled
    if get_pick_head(&state.repo)?.is_some() && state.config.advice.cherry_pick {
      let confirmed = get_user_confirmation(CONFIRM_DURING_PICK)?;
      if !confirmed {
        println!("Cancelled commit");
        return Ok(());
      }
    }

    // if there's a revert active and the user has revert advice enabled
    if get_revert_head(&state.repo)?.is_some() && state.config.advice.revert {
      let confirmed = get_user_confirmation(CONFIRM_DURING_REVERT)?;
      if !confirmed {
        println!("Cancelled commit");
        return Ok(());
      }
    }

    let target = match &self.to {
      Some(to) => {
        let reference = state.repo.resolve_reference_from_short_name(to)?;

        Some(CommitTarget {
          commit: reference.peel_to_commit()?,
          display_name: reference.shorthand_bytes().to_str_lossy_owned(),
          refname: Some(reference.name_bytes().to_str_lossy_owned()),
        })
      }

      None => match get_head(&state.repo)? {
        Some(head) => Some(CommitTarget {
          commit: head.peel_to_commit()?,
          display_name: head.shorthand_bytes().to_str_lossy_owned(),
          refname: Some("HEAD".to_string()),
        }),

        None => None,
      },
    };

    if self.amend {
      return self.amend(state, target);
    }

    let commit_type = get_commit_type(&state.repo);

    // get starter msg
    let mut msg = {
      let cli_msg = self.words.join(" ");

      if !cli_msg.is_empty() {
        cli_msg
      } else {
        get_initial_msg(&state.repo, &commit_type)?
      }
    };

    if commit_type == CommitType::Normal {
      // if it's a normal commit, require non-empty changes
      if !has_index_changes(&state.repo)? {
        return Err(anyhow!(
          "Nothing to commit! Stage some changes with \"git add/rm …\""
        ));
      }
    }

    self.pre_commit(&state.repo)?;

    let (tree, diff) = self.get_changes(
      &state.repo,
      match &target {
        Some(target) => Some(&target.commit),
        None => None,
      },
    )?;
    let sig = get_signature(&state.repo)?.ok_or(anyhow!(NO_SIGNATURE_MSG))?;

    // get final msg
    if self.edit {
      msg = self.invoke_editor(
        &state.repo,
        &build_msg_template(
          msg.as_bytes(),
          target.as_ref().map(|it| it.display_name.as_str()),
          &diff,
        )?,
      )?;
    } else if msg.trim().is_empty() {
      return Err(anyhow!("Must specify a commit message!"));
    }

    let parents = self.get_parents(&state.repo, target.as_ref().map(|it| it.commit.clone()))?;
    let parents: Vec<&Commit<'_>> = parents.iter().collect();

    let new_id = state
      .repo
      .commit(
        match &target {
          Some(target) => target.refname.as_deref(),
          // empty repo, just update head
          None => Some("HEAD"),
        },
        &sig,
        &sig,
        &msg,
        &tree,
        &parents,
      )
      .context("Failed to commit")?;

    let merge_head = get_merge_head(&state.repo)?;
    if let Some(merge_head) = &merge_head {
      println!(
        "{}",
        display_merge_header(
          &state.repo,
          merge_head,
          target.as_ref().map(|it| it.display_name.as_str())
        )?
      );
    } else {
      let target_name = match &target {
        Some(target) => Some(target.display_name.clone()),
        None => get_current_branch_name(&state.repo)?,
      };
      println!("{}", display_commit_header(target_name.as_deref())?);
    };

    let new_commit = state.repo.find_commit(new_id)?;

    println!(
      "{}",
      display_commit_details(&new_commit, &diff, &state.config)?
    );

    // committing during an active merge completes the merge, we should clean up the merge files
    if merge_head.is_some() {
      state.repo.cleanup_state()?;
      println!("\n{}", style("Merge completed!").dim())
    }

    Ok(())
  }

  fn amend(&self, state: &App, target: Option<CommitTarget>) -> Result<()> {
    let target = target.ok_or(anyhow!("No current commit to amend"))?;
    self.pre_commit(&state.repo)?;

    let (tree, diff) = self.get_changes(&state.repo, Some(&target.commit))?;
    let sig = state.repo.signature().context(NO_SIGNATURE_MSG)?;

    // get commit msg
    let cli_msg = self.words.join(" ");
    let msg = if self.edit {
      let old_msg = target.commit.message_bytes();
      Some(self.invoke_editor(
        &state.repo,
        &build_msg_template(old_msg, Some(&target.display_name), &diff)?,
      )?)
    } else if !cli_msg.is_empty() {
      Some(cli_msg)
    } else {
      None // use existing msg
    };

    // amend the commit
    let new_id = target
      .commit
      .amend(
        target.refname.as_deref(),
        if self.reset_author { Some(&sig) } else { None },
        Some(&sig),
        None,
        msg.as_deref(),
        Some(&tree),
      )
      .context("Failed to amend commit")?;

    println!(
      "{}",
      display_amend_header(&target.commit, &target.display_name)?
    );

    let new_commit = state.repo.find_commit(new_id)?;
    println!(
      "{}",
      display_commit_details(&new_commit, &diff, &state.config)?
    );
    Ok(())
  }

  /// Computes the changes to commit.
  ///
  /// # Returns
  /// A tuple containing the tree to apply and a diff of the changes
  fn get_changes<'repo>(
    &self,
    repo: &'repo Repository,
    target: Option<&Commit>,
  ) -> Result<(Tree<'repo>, Diff<'repo>)> {
    // compute changes to commit
    let head = get_head(repo)?;
    let base_tree = match head {
      Some(head) => Some(head.peel_to_tree()?),
      None => None,
    };
    let target_tree = match target {
      Some(target) => Some(target.tree()?),
      None => None,
    };

    let mut index = repo.index()?;
    let index_tree = {
      let id = index.write_tree()?;
      repo.find_tree(id)?
    };

    let mut opts = DiffOptions::new();
    opts.context_lines(0); // context lines will cause conflicts most of the time
    let mut changes =
      repo.diff_tree_to_tree(base_tree.as_ref(), Some(&index_tree), Some(&mut opts))?;

    let Some(target_tree) = target_tree else {
      changes.find_similar(None)?;
      return Ok((index_tree, changes));
    };

    let mut index = match repo.apply_to_tree(&target_tree, &changes, None) {
      Ok(index) => Ok(index),
      Err(e) if e.code() == ErrorCode::ApplyFail => Err(anyhow!(
        "The staged changes cannot be committed because they would result in a conflict.\
        Check with \"git diff --cached{}\", and adjust staged changes accordingly.",
        match &self.to {
          Some(to) => format!(" {}", to),
          None => "".to_string(),
        }
      )),
      Err(e) => Err(e.into()),
    }?;
    let id = index.write_tree_to(repo)?;

    changes.find_similar(None)?;
    Ok((repo.find_tree(id)?, changes))
  }

  /// Gets the parents of the new commit
  fn get_parents<'repo>(
    &self,
    repo: &'repo Repository,
    target: Option<Commit<'repo>>,
  ) -> Result<Vec<Commit<'repo>>> {
    let mut parents: Vec<Commit> = target.into_iter().collect();

    // if MERGE_HEAD exists, make sure to add it as a parent
    if let Some(merge_head) = get_merge_head(repo)? {
      let merge_commit = merge_head.peel_to_commit()?;
      parents.push(merge_commit);
    }

    Ok(parents)
  }

  fn pre_commit(&self, repo: &Repository) -> Result<()> {
    if self.no_verify {
      println!("{}", style("Skipping precommit hook").yellow());
      let _ = std::io::stdout().flush(); // flush is for ux, but isn't a big deal if it fails
      return Ok(());
    }

    let git_dir = repo.path();
    let script = git_dir.join("hooks").join("pre-commit");

    if !script.exists() {
      // no hooks set, always succeed
      return Ok(());
    }

    print!("Running precommit hook…");
    let _ = std::io::stdout().flush();

    let output = Command::new(script).output()?;

    if output.status.success() {
      println!(" {}", style("passed!").green());
      let _ = std::io::stdout().flush();
      Ok(())
    } else {
      println!(" {}", style("failed!").red());
      let _ = std::io::stdout().flush();
      eprintln!("Precommit output:");
      eprintln!();
      eprintln!("{}", output.stderr.to_str_lossy());
      Err(anyhow!("Precommit hook failed"))
    }
  }

  /// Invokes the commit message editor and returns the message
  fn invoke_editor(&self, repo: &Repository, template: &[u8]) -> Result<String> {
    let editor = get_editor(repo)?;
    let editmsg = repo.path().join("COMMIT_EDITMSG");

    // initialize with template msg
    fs::write(&editmsg, template)?;

    // run the editor in a shell to parse args
    #[cfg(unix)]
    let status = Command::new("sh")
      .arg("-c")
      .arg(format!("{} \"{}\"", editor, &editmsg.to_string_lossy()))
      .status()?;

    #[cfg(windows)]
    let status = Command::new("cmd")
      .args([
        "/C",
        &format!("{} \"{}\"", editor, &editmsg.to_string_lossy()),
      ])
      .status()?;

    if !status.success() {
      return Err(if let Some(code) = status.code() {
        anyhow!("Editor failed with code: {}", code)
      } else {
        anyhow!("Editor failed")
      });
    }

    // read the edited msg
    let msg = fs::read_to_string(editmsg)?;

    // remove comment lines
    let mut out = String::with_capacity(msg.len());
    for line in msg.lines() {
      if !line.starts_with('#') {
        out.push_str(line);
      }
    }

    Ok(out)
  }
}

#[derive(PartialEq, Eq)]
enum CommitType {
  Squash,
  Merge,
  /// Needs to store which rebase dir was found (rebase-merge or rebase-apply)
  Rebase(String),
  Normal,
}

fn get_commit_type(repo: &Repository) -> CommitType {
  let git_dir = repo.path();

  if git_dir.join("SQUASH_MSG").exists() {
    return CommitType::Squash;
  }

  if git_dir.join("MERGE_HEAD").exists() {
    return CommitType::Merge;
  }

  if git_dir.join("rebase-merge").exists() {
    return CommitType::Rebase("rebase-merge".into());
  }

  if git_dir.join("rebase-apply").exists() {
    return CommitType::Rebase("rebase-apply".into());
  }

  CommitType::Normal
}

/// Finds the configured editor, matching git's search order
fn get_editor(repo: &Repository) -> Result<String> {
  // 1. GIT_EDITOR env var
  match env::var("GIT_EDITOR") {
    Ok(it) => return Ok(it),
    Err(VarError::NotPresent) => {}
    Err(e) => return Err(e.into()),
  };

  // 2. core.editor config var
  let config = repo.config()?;
  match config.get_string("core.editor") {
    Ok(it) => return Ok(it),
    Err(e) if e.code() == ErrorCode::NotFound => {}
    Err(e) => return Err(e.into()),
  };

  // 3, 4. VISUAL, EDITOR env vars
  for var in ["VISUAL", "EDITOR"] {
    match env::var(var) {
      Ok(it) => return Ok(it),
      Err(VarError::NotPresent) => {}
      Err(e) => return Err(e.into()),
    };
  }

  // 5. platform default
  Ok(if cfg!(windows) {
    "notepad".into()
  } else {
    "vi".into()
  })
}

/// Gets the default commit message depending on the repository state
fn get_initial_msg(repo: &Repository, commit_type: &CommitType) -> Result<String> {
  let git_dir = repo.path();

  Ok(match commit_type {
    CommitType::Squash => fs::read_to_string(git_dir.join("SQUASH_MSG"))?,
    CommitType::Merge => fs::read_to_string(git_dir.join("MERGE_MSG"))?,
    CommitType::Rebase(path) => fs::read_to_string(git_dir.join(path).join("message"))?,
    CommitType::Normal => "\n".to_string(),
  })
}

/// Builds the content of COMMIT_EDITMSG before it's opened in an editor
///
/// # Params
/// - `initial` - the pre-filled commit message (should usually end with a newline)
/// - `to` - a user-friendly name for where the commit is being applied. None when there are no commits in the repo
/// - `diff` - the changes in the commit
///
/// # Returns
/// The text that should populate COMMIT_EDITMSG
fn build_msg_template(initial: &[u8], to: Option<&str>, diff: &Diff) -> Result<Vec<u8>> {
  let mut out: Vec<u8> = Vec::with_capacity(1000);
  out.extend_from_slice(initial);

  if let Some(to) = to {
    out.extend_from_slice(format!("\n# Committing on {}", to).as_bytes());
  } else {
    out.extend_from_slice(b"\n# Initial commit");
  }

  let summary = DiffSummary::new(diff)?;
  let summary = summary.to_string();

  for line in summary.lines() {
    out.extend_from_slice(format!("\n# {}", strip_ansi_codes(line)).as_bytes());
  }

  Ok(out)
}

/// Displays the header-line for a regular commit
///
/// Committed hash to branch as Author Name
fn display_commit_header(target: Option<&str>) -> Result<String> {
  Ok(format!(
    "{} to {}",
    style("Committed").green(),
    match target {
      Some(it) => style(it).blue(),
      None => style("unknown").red(),
    }
  ))
}

/// Displays the header-line for an amend
///
/// `Amended <old hash> on <branch> as <Author Name>`
fn display_amend_header(old_commit: &Commit, target: &str) -> Result<String> {
  Ok(format!(
    "{} {} on {}",
    style("Amended").green(),
    display_hash(old_commit)?,
    style(target).blue()
  ))
}

/// Displays the header line for a merge commit
///
/// `Merged <base> into <branch>: <hash> as <Author Name>`
fn display_merge_header(
  repo: &Repository,
  merge_head: &Reference,
  head: Option<&str>,
) -> Result<String> {
  let merge_commit = merge_head.peel_to_commit()?;
  let from = style(resolve_commit_name(repo, &merge_commit)?).blue();

  let to = match head {
    Some(it) => style(it).magenta().to_string(),
    None => get_current_branch_name(repo)?
      .map(|it| style(it).magenta().to_string())
      .unwrap_or(style("unknown").red().to_string()),
  };

  Ok(format!("{} {} into {}", style("Merged").green(), from, to))
}

/// Displays the remaining commit details in the same format as `feature show`, with two exceptions:
/// 1. The time is always absolute
/// 2. It always displays the entire commit message
fn display_commit_details(commit: &Commit<'_>, diff: &Diff, config: &Config) -> Result<String> {
  let commit_output = display_commit(
    commit,
    &DisplayCommitOptions {
      time: DisplayTimeOptions {
        // relative is not useful, commit just occured
        relative: false,
        date: config.format.date,
        hour: config.format.hour,
        timezone: config.format.timezone,
      },
      // want the user to see the entire message just for reference
      message: DisplayCommitMessageLevel::Full,
    },
  )?;

  let summary = DiffSummary::new(diff)?;
  Ok(format!("{}\n\n{}", commit_output, summary))
}
