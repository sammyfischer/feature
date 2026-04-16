//! Commit subcommand

use std::io::Write;
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use console::style;
use git2::{Commit, Diff, Oid, Reference, Repository};

use crate::config::Config;
use crate::util::advice::NO_SIGNATURE_MSG;
use crate::util::branch::{
  get_current_branch_or_commit,
  get_head,
  get_merge_head,
  get_pick_head,
  get_revert_head,
  resolve_branch_name,
};
use crate::util::diff::DiffSummary;
use crate::util::display::{
  DisplayCommitMessageLevel,
  DisplayCommitOptions,
  DisplayTimeOptions,
  display_commit,
  display_hash,
};
use crate::util::term::get_user_confirmation;
use crate::util::{get_signature, read_commit_msg, resolve_commit_name};
use crate::{App, lossy};

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

struct CommitTarget<'repo> {
  commit: Commit<'repo>,
  /// Something user-friendly to print (ideally branch name, maybe tag or short hash)
  display_name: String,
  /// The ref to update. Will be None if we're not committing to a branch
  refname: Option<String>,
}

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Commit staged changes")]
pub struct Args {
  /// Whether to amend the previous commit
  #[arg(long, long_help = AMEND_LONG_HELP)]
  amend: bool,

  /// Where to apply the commit. Can be anything commit-ish
  #[arg(long)]
  to: Option<String>,

  /// Bypass precommit hooks
  #[arg(long)]
  no_verify: bool,

  /// Words to join together as commit message
  #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
  words: Vec<String>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let mut msg = self.words.join(" ");

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
        let object = state.repo.revparse_single(to)?;
        let commit = object.peel_to_commit()?;
        let display_name = resolve_commit_name(&state.repo, &commit.id())?;

        Some(match resolve_branch_name(&state.repo, to)? {
          Some(branch) => CommitTarget {
            commit,
            display_name,
            refname: Some(lossy!(branch.get().name_bytes()).to_string()),
          },
          None => CommitTarget {
            commit,
            display_name,
            refname: None,
          },
        })
      }

      None => match get_head(&state.repo)? {
        Some(head) => Some(CommitTarget {
          commit: head.peel_to_commit()?,
          display_name: lossy!(head.shorthand_bytes()).to_string(),
          refname: Some("HEAD".to_string()),
        }),

        None => None,
      },
    };

    let signature = get_signature(&state.repo)?.ok_or(anyhow!(NO_SIGNATURE_MSG))?;
    let mut index = state.repo.index().context("Failed to get staged changes")?;

    let index_tree_id = index.write_tree().context("Failed to get index tree")?;
    let index_tree = state
      .repo
      .find_tree(index_tree_id)
      .context("Failed to get index tree")?;

    // all the info needed for amend
    if self.amend {
      let target = target.ok_or(anyhow!("No commits yet, cannot amend"))?;

      self.pre_commit(&state.repo)?;

      let new_id = target
        .commit
        .amend(
          target.refname.as_deref(),
          None,
          Some(&signature),
          None,
          if !msg.is_empty() { Some(&msg) } else { None },
          Some(&index_tree),
        )
        .expect("Failed to amend commit");

      println!(
        "{}",
        display_amend_header(&target.commit.id(), &target.display_name)?
      );

      let new_commit = state.repo.find_commit(new_id)?;
      let mut diff = state.repo.diff_tree_to_tree(
        Some(&target.commit.tree()?),
        Some(&new_commit.tree()?),
        None,
      )?;
      diff.find_similar(None)?;

      println!(
        "{}",
        display_commit_details(&new_commit, &diff, &state.config)?
      );
      return Ok(());
    }

    let merge_head = get_merge_head(&state.repo)?;

    if merge_head.is_none() {
      // if it's not a merge, require non-empty commit
      // note: merge commits can appear empty wrt the target commit, since they may resolve
      // conflicts to look exactly like the target
      let target_tree = target.as_ref().and_then(|it| it.commit.tree().ok());

      let staged_diff = state
        .repo
        .diff_tree_to_index(target_tree.as_ref(), Some(&index), None)
        .context("Failed to analyze staged changes")?;

      let staged_stats = staged_diff
        .stats()
        .context("Failed to analyze staged changes")?;

      if staged_stats.files_changed() == 0 {
        return Err(anyhow!(
          r#"Nothing to commit! Stage some changes with "git add …""#
        ));
      }
    }

    if msg.is_empty() {
      // if it's a merge, try to get the msg from .git/MERGE_MSG
      'merge_msg: {
        if merge_head.is_some() {
          let path = state.repo.path().join("MERGE_MSG");

          // if not found, default
          if path.exists() {
            let merge_msg = read_commit_msg(&path)
              .context("Failed to get default merge message. Try specifying a message manually.")?;

            // if not empty, use it
            if !merge_msg.is_empty() {
              msg = merge_msg.to_string();
              // break to avoid error since we found the message
              break 'merge_msg;
            }
          }
        }

        // if we didn't break, fall through and error
        return Err(anyhow!("Must specify a commit message"));
      }
    }

    let old_tree = match &target {
      Some(it) => Some(it.commit.tree()?),
      None => None,
    };

    let mut parent_commits: Vec<&Commit> =
      target.as_ref().map(|it| &it.commit).into_iter().collect();

    let merge_commit_list: Vec<Commit> = match merge_head.as_ref() {
      Some(it) => it.peel_to_commit().into_iter().collect(),
      None => Vec::new(),
    };

    for merge_commit in &merge_commit_list {
      parent_commits.push(merge_commit);
    }

    self.pre_commit(&state.repo)?;

    let new_id = state
      .repo
      .commit(
        match &target {
          Some(target) => target.refname.as_deref(),
          // empty repo, just update head
          None => Some("HEAD"),
        },
        &signature,
        &signature,
        &msg,
        &index_tree,
        &parent_commits,
      )
      .expect("Failed to commit");

    if let Some(merge_head) = &merge_head {
      println!(
        "{}",
        display_merge_header(
          &state.repo,
          merge_head,
          &get_current_branch_or_commit(&state.repo)?
            .expect("There should be a current commit after merging")
        )?
      );
    } else {
      let target_name = match target {
        Some(target) => target.display_name,
        None => get_current_branch_or_commit(&state.repo)?
          .context("There should be a current commit after committing")?,
      };
      println!("{}", display_commit_header(&target_name)?);
    };

    let new_commit = state.repo.find_commit(new_id)?;
    let mut diff =
      state
        .repo
        .diff_tree_to_tree(old_tree.as_ref(), Some(&new_commit.tree()?), None)?;
    diff.find_similar(None)?;

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
      eprintln!("{}", lossy!(&output.stderr));
      Err(anyhow!("Precommit hook failed"))
    }
  }
}

/// Displays the header-line for a regular commit
///
/// Committed hash to branch as Author Name
fn display_commit_header(target: &str) -> Result<String> {
  use std::fmt::Write;
  let mut out = String::with_capacity(80);

  write!(out, "{}", style("Committed").green())?;
  write!(out, " to {}", style(target).blue())?;

  Ok(out)
}

/// Displays the header-line for an amend
///
/// Amended oldhash on branch as Author Name
fn display_amend_header(old_id: &Oid, target: &str) -> Result<String> {
  use std::fmt::Write;
  let mut out = String::with_capacity(80);

  write!(out, "{}", style("Amended").green())?;
  write!(out, " {}", display_hash(old_id))?;
  write!(out, " on {}", style(target).blue())?;

  Ok(out)
}

/// Displays the header line for a merge commit
///
/// Merged base into branch: hash as Author Name
fn display_merge_header(repo: &Repository, merge_head: &Reference, head: &str) -> Result<String> {
  use std::fmt::Write;
  let mut out = String::with_capacity(80);

  let merge_commit = merge_head.peel_to_commit()?;
  let from = resolve_commit_name(repo, &merge_commit.id())?;

  write!(out, "{}", style("Merged").green())?;
  write!(
    out,
    " {} into {}",
    style(from).blue(),
    style(head).magenta()
  )?;

  Ok(out)
}

/// Displays the remaining commit details. This is the same for all types of commits (regular,
/// amend, merge).
///
/// Params
/// - `msg` - The entire commit message. This may be empty.
/// - `diff` - A diff to display. The diff depends on the commit type:
///   - regular: (first) parent to commit
///   - amend: old to new
///   - merge: first parent to commit (changes introduced by merge)
fn display_commit_details(commit: &Commit<'_>, diff: &Diff, config: &Config) -> Result<String> {
  use std::fmt::Write;
  let mut out = String::with_capacity(200);

  write!(
    out,
    "{}",
    display_commit(commit, &DisplayCommitOptions {
      time: DisplayTimeOptions {
        // relative is not useful, commit just occured
        relative: false,
        date: config.format.date,
        hour: config.format.hour,
        timezone: config.format.timezone
      },
      // want the user to see the entire message just for reference
      message: DisplayCommitMessageLevel::Full
    })?
  )?;

  let summary = DiffSummary::new(diff);

  write!(out, "\n\n{}", match summary {
    Ok(it) => it.to_string(),
    Err(_) => style("Failed to get commit changes").red().to_string(),
  })?;
  Ok(out)
}
