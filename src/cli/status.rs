use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use console::{measure_text_width, pad_str, style, truncate_str};
use git2::{DiffOptions, Oid, Reference, Repository};

use crate::util::advice::{
  BISECT_ADVICE,
  MERGE_CONFLICT_ADVICE,
  PICK_CONFLICT_ADVICE,
  REBASE_CONFLICT_ADVICE,
  REVERT_CONFLICT_ADVICE,
  STATUS_ADVICE,
};
use crate::util::branch::{
  commit_to_branch,
  get_ahead_behind,
  get_current_branch_or_commit,
  get_head,
  get_merge_head,
  get_pick_head,
  get_revert_head,
};
use crate::util::branch_meta::BranchMeta;
use crate::util::diff::DiffSummary;
use crate::util::display::{display_commit_compact, display_plus_minus, display_signature};
use crate::util::get_signature;
use crate::util::lossy::{ToStrLossy, ToStrLossyOwned};
use crate::util::term::{get_term_width, is_term};
use crate::{App, data, opt_advice};

#[derive(clap::Args, Clone, Debug)]
#[command(
  visible_alias = "st",
  about = "View current status (current branch, author info, changes)"
)]
pub struct Args {
  /// Hides untracked files from output
  #[arg(short = 'u', long, value_name = "HIDE")]
  pub hide_untracked: bool,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let head = get_head(&state.repo)?;
    let rebase_dir = get_rebase_dir(&state.repo);

    let (header, advice) = if let Some(dir) = rebase_dir.as_ref() {
      (
        display_rebase_header(&state.repo, dir)?,
        opt_advice!(state.config.advice.rebase, REBASE_CONFLICT_ADVICE),
      )
    } else if is_merge_active(&state.repo) {
      (
        display_merge_header(&state.repo)?,
        opt_advice!(state.config.advice.merge, MERGE_CONFLICT_ADVICE),
      )
    } else if is_pick_active(&state.repo) {
      (
        display_pick_header(&state.repo)?,
        opt_advice!(state.config.advice.cherry_pick, PICK_CONFLICT_ADVICE),
      )
    } else if is_revert_active(&state.repo) {
      (
        display_revert_header(&state.repo)?,
        opt_advice!(state.config.advice.revert, REVERT_CONFLICT_ADVICE),
      )
    } else if is_bisect_active(&state.repo) {
      (
        display_bisect_header(&state.repo)?,
        opt_advice!(state.config.advice.bisect, BISECT_ADVICE),
      )
    } else {
      (
        display_normal_header(state, head.as_ref())?,
        opt_advice!(state.config.advice.status, STATUS_ADVICE),
      )
    };

    println!("{}", header);

    // signature/author info
    println!(
      "{}",
      display_signature(get_signature(&state.repo)?.as_ref())
    );

    // print advice in new paragraph above diffs
    if let Some(advice) = advice {
      println!("\n{}", advice);
    }

    // get current tree to diff from
    let tree = match &head {
      Some(head) => {
        let commit = head.peel_to_commit()?;
        Some(commit.tree()?)
      }
      None => None,
    };

    // conflicted changes
    if rebase_dir.is_some()
      || is_merge_active(&state.repo)
      || is_pick_active(&state.repo)
      || is_revert_active(&state.repo)
    {
      let tree = tree
        .as_ref()
        .context("There must be a current commit during a rebase")?;

      let diff = state.repo.diff_tree_to_index(Some(tree), None, None)?;
      let summary = DiffSummary::new(&diff)?.conflicts();

      println!(
        "\n{} - {}",
        style("Conflicts").yellow(),
        if summary.num_files != 0 {
          summary.display_conflicts()
        } else {
          style("none").green().to_string()
        }
      );
    }

    if is_pick_active(&state.repo) {
      // cherry picks are weird bc they show no diff with head when you stage changes. to show
      // meaningful changes you have to diff with the picked commit
      let pick_head = state.repo.find_reference("CHERRY_PICK_HEAD")?;
      let pick_tree = pick_head.peel_to_tree()?;

      let diff = state
        .repo
        .diff_tree_to_index(Some(&pick_tree), None, None)?;
      let summary = DiffSummary::new(&diff)?.non_conflicts();

      if summary.num_files != 0 {
        println!("\n{} - {}", style("Resolved").green(), summary);
      }
      // cherry picked changes have no difference with head (except for conflicts), so the remaining
      // diffs can be skipped
      return Ok(());
    }

    // staged changes
    let mut diff = state
      .repo
      .diff_tree_to_index(tree.as_ref(), None, None)
      .context("Failed to get staged changes")?;
    diff.find_similar(None)?;

    match DiffSummary::new(&diff) {
      Ok(summary) => {
        // filter out conflicted files
        let summary = summary.non_conflicts();
        if summary.num_files != 0 {
          println!("\n{} - {}", style("Staged").green(), summary);
        }
      }
      Err(_) => {
        println!("\nFailed to get summary of staged changes");
      }
    };

    // unstaged changes
    let mut opts = if self.hide_untracked || !state.config.status.show_untracked {
      None
    } else {
      let mut opts = DiffOptions::new();
      opts.include_untracked(true);
      Some(opts)
    };

    let mut diff = state
      .repo
      .diff_index_to_workdir(None, opts.as_mut())
      .context("Failed to get unstaged changes")?;
    diff.find_similar(None)?;

    match DiffSummary::new(&diff) {
      Ok(it) => {
        let it = it.non_conflicts();
        if it.num_files != 0 {
          println!();
          print!("{} - ", style("Unstaged").red());
          println!("{}", it)
        }
      }
      Err(_) => {
        println!();
        println!("Failed to get summary of unstaged changes");
      }
    };

    Ok(())
  }
}

/// Displays a header when there is no other active operation (e.g. rebase/merge conflicts). Shows
/// current branch, commit it points to, and upstream/base info if available. Unlike the others,
/// this header takes up to 3 lines.
fn display_normal_header(state: &App, head: Option<&Reference>) -> Result<String> {
  let mut out = String::with_capacity(80);
  let mut branch = None;

  let first_line = match head {
    // there are commits in the repo
    Some(head) => {
      let commit = head
        .peel_to_commit()
        .context("Failed to get commit at HEAD")?;

      // display branch name or detached head indicator
      let display_branch = if head.is_branch() {
        let meta = BranchMeta::from_reference(head)?;
        let display = format!("On {}", style(meta.name()).green());
        branch = Some(meta);
        display
      } else {
        style("Detached HEAD").red().to_string()
      };

      format!("{} -> {}", display_branch, display_commit_compact(&commit)?)
    }

    // head points to nothing, no commits in repo
    None => {
      let head = state.repo.find_reference("HEAD")?;
      let symbolic_ref = head
        .symbolic_target_bytes()
        .expect("HEAD points to nothing. Is the .git/HEAD file corrupt or missing?")
        .to_str_lossy();
      format!(
        "On {}, no commits yet",
        style(symbolic_ref.trim_prefix("refs/heads/")).green()
      )
    }
  };

  // end first line
  if is_term() {
    write!(
      out,
      "{}",
      truncate_str(&first_line, get_term_width(), &style("…").dim().to_string())
    )?;
  } else {
    write!(out, "{}", &first_line)?;
  }

  // upstream and base ahead/behind if we're on a branch
  if head.is_some_and(|it| it.is_branch()) {
    let branch = branch.expect("Should be checked out to a branch");
    // we don't fetch, so we can use this ref multiple times
    let branch_ref = branch.resolve(&state.repo)?;

    let mut rows: Vec<[String; 2]> = Vec::with_capacity(2);
    // the label is either "Upstream" or "Base", these are printed with alignment so the branch
    // names are lined up
    let mut label_width = 0usize;

    // upstream row
    let upstream = branch.upstream(&state.repo)?;
    if let Some(upstream) = upstream {
      let upstream = BranchMeta::from_branch(&upstream)?;
      let (a, b) = get_ahead_behind(&state.repo, &branch_ref, &upstream.resolve(&state.repo)?)
        .context("Failed to get ahead/behind for upstream")?;

      let row = [
        style("Upstream").blue().to_string(),
        format!(
          "{}{} {}{}",
          style("[").dim(),
          style(upstream.name()),
          display_plus_minus(a, b),
          style("]").dim(),
        ),
      ];
      label_width = measure_text_width(&row[0]);
      rows.push(row);
    }

    // base row
    let base = data::get_feature_base(&state.repo, branch.name())?;
    if let Some(base) = base {
      let (a, b) = get_ahead_behind(&state.repo, &branch_ref, &base.resolve(&state.repo)?)
        .context("Failed to get ahead/behind for base")?;

      let row = [
        style("Base").magenta().to_string(),
        format!(
          "{}{} {}{}",
          style("[").dim(),
          style(base.name()),
          display_plus_minus(a, b),
          style("]").dim(),
        ),
      ];
      label_width = label_width.max(measure_text_width(&row[0]));
      rows.push(row);
    }

    // print with everything after the row label aligned
    for row in rows {
      write!(
        out,
        "\n  {} {}",
        pad_str(&row[0], label_width, console::Alignment::Left, None),
        &row[1]
      )?;
    }
  }

  Ok(out)
}

fn get_rebase_dir(repo: &Repository) -> Option<PathBuf> {
  let rebase_merge = repo.path().join("rebase-merge");
  let rebase_apply = repo.path().join("rebase-apply");
  let dir = if rebase_merge.exists() {
    rebase_merge
  } else if rebase_apply.exists() {
    rebase_apply
  } else {
    return None;
  };
  Some(dir)
}

/// Displays a header line for an active rebase. Includes the source and destination branches, and
/// the current progress.
fn display_rebase_header(repo: &Repository, dir: &Path) -> Result<String> {
  let msgnum =
    fs::read_to_string(dir.join("msgnum")).context("Failed to get current step number")?;
  let current = msgnum.trim();

  let end = fs::read_to_string(dir.join("end")).context("Failed to get total number of steps")?;
  let total = end.trim();

  let progress = format!(
    "{}{}/{}{}",
    style("[").dim(),
    current,
    total,
    style("]").dim()
  );

  let head_name_path = dir.join("head-name");
  let head_name = fs::read_to_string(&head_name_path).context("Failed to get branch name")?;
  let branch_ref = repo.resolve_reference_from_short_name(head_name.trim())?;
  let branch_name = branch_ref.shorthand_bytes().to_str_lossy();

  let onto_path = dir.join("onto");
  let onto = fs::read_to_string(&onto_path).context("Failed to get base commit")?;
  let onto = onto.trim();

  // this must be parseable as an id
  let base_id = Oid::from_str(onto).with_context(|| {
    format!(
      "{} should contain a valid commit hash",
      onto_path.to_string_lossy()
    )
  })?;
  let base_commit = repo.find_commit(base_id)?;

  // try to find a matching branch, but don't error
  let base = match commit_to_branch(repo, &base_id) {
    Ok(branch) => match branch {
      Some(branch) => match branch.name_bytes() {
        Ok(name) => Some(name.to_str_lossy_owned()),
        Err(_) => None,
      },
      None => None,
    },
    Err(_) => None,
  }
  // if all else fails, use the short hash
  .unwrap_or(base_commit.as_object().short_id()?.to_str_lossy_owned());

  Ok(format!(
    "{} {} onto {} {}",
    style("Rebasing").yellow(),
    style(&branch_name).blue(),
    style(&base).magenta(),
    progress
  ))
}

fn is_merge_active(repo: &Repository) -> bool {
  repo.path().join("MERGE_HEAD").exists()
}

/// Displays a summary of an ongoing merge
fn display_merge_header(repo: &Repository) -> Result<String> {
  let merge_head = get_merge_head(repo)?.context("Reference MERGE_HEAD does not exist")?;
  let merge_commit = merge_head.peel_to_commit()?;

  // current branch if it was detected, else current commit
  let current = get_current_branch_or_commit(repo)?
    .expect("HEAD should point to a commit during an active merge");

  // get the branch pointed to by MERGE_HEAD, else just use the hash
  let base = match commit_to_branch(repo, &merge_commit.id())? {
    Some(branch) => match branch.name_bytes() {
      Ok(name) => name.to_str_lossy_owned(),
      Err(_) => "unknown".to_string(),
    },
    None => merge_commit.as_object().short_id()?.to_str_lossy_owned(),
  };

  Ok(format!(
    "{} {} with {}",
    style("Merging").yellow(),
    style(current).blue(),
    style(base).magenta()
  ))
}

fn is_pick_active(repo: &Repository) -> bool {
  repo.path().join("CHERRY_PICK_HEAD").exists()
}

/// Displays a header line for an active cherry-pick conflict
fn display_pick_header(repo: &Repository) -> Result<String> {
  let pick_head = get_pick_head(repo)?.context("Reference CHERRY_PICK_HEAD does not exist")?;
  let pick_commit = pick_head.peel_to_commit()?;

  let current = get_current_branch_or_commit(repo)?
    .expect("HEAD should point to a commit during an active cherry-pick");

  Ok(format!(
    "{} {} onto {}",
    style("Picking").yellow(),
    style(pick_commit.as_object().short_id()?.to_str_lossy()).blue(),
    style(current).magenta()
  ))
}

fn is_revert_active(repo: &Repository) -> bool {
  repo.path().join("REVERT_HEAD").exists()
}

fn display_revert_header(repo: &Repository) -> Result<String> {
  let revert_head = get_revert_head(repo)?.context("Reference REVERT_HEAD does not exist")?;
  let revert_commit = revert_head.peel_to_commit()?;

  // current branch if it was detected, else current commit
  let current = get_current_branch_or_commit(repo)?
    .expect("HEAD should point to a commit during an active revert");

  Ok(format!(
    "{} changes from {} onto {}",
    style("Reverting").yellow(),
    style(revert_commit.as_object().short_id()?.to_str_lossy()).blue(),
    style(current).magenta()
  ))
}

fn is_bisect_active(repo: &Repository) -> bool {
  let dir = repo.path();
  dir.join("BISECT_START").exists() || dir.join("BISECT_LOG").exists()
}

fn display_bisect_header(repo: &Repository) -> Result<String> {
  let current = get_current_branch_or_commit(repo)?
    .expect("HEAD should point to a commit during an active bisect");

  let start_path = repo.path().join("BISECT_START");
  let mut start = fs::read_to_string(&start_path)?.trim().to_string();

  if let Ok(id) = Oid::from_str(&start) {
    let commit = repo.find_commit(id)?;
    start = commit.as_object().short_id()?.to_str_lossy_owned();
  }

  Ok(format!(
    "{} on {} {}",
    style("Bisecting").yellow(),
    style(&current).blue(),
    style(&format!("(started from {})", start)).dim()
  ))
}
