use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use console::{measure_text_width, pad_str, style, truncate_str};
use git2::{DiffOptions, Oid, Reference, Repository};

use crate::cli::Cli;
use crate::util::branch::{
  branch_to_name,
  commit_to_branch,
  get_ahead_behind,
  get_current_branch_name,
  get_head,
  get_upstream,
  name_to_branch,
};
use crate::util::diff::DiffSummary;
use crate::util::display::{display_hash, display_plus_minus, display_signature, trim_hash};
use crate::util::term::{get_term_width, is_term};
use crate::util::{get_current_commit, get_signature};
use crate::{data, lossy, open_repo};

#[derive(clap::Args, Clone, Debug)]
#[command(
  visible_alias = "st",
  about = "View current status (current branch, author info, changes)"
)]
pub struct Args {
  /// Hides untracked files from output
  #[arg(short = 'u', long)]
  pub hide_untracked: bool,
}

impl Args {
  pub fn run(&self, cli: &Cli) -> Result<()> {
    let repo = open_repo!();
    let head = get_head(&repo)?;

    let rebase_dir = get_rebase_dir(&repo);

    // display header line depending on current state
    if let Some(dir) = rebase_dir.as_ref() {
      // active rebase
      println!("{}", display_rebase_header(&repo, dir)?);
    } else if is_merge_active(&repo) {
      // active merge
      println!("{}", display_merge_header(&repo)?);
    } else {
      // nothing else
      println!("{}", display_normal_header(&repo, head.as_ref())?);
    }

    // signature/author info
    println!("{}", display_signature(get_signature(&repo)?.as_ref()));

    // get current tree to diff from
    let tree = match &head {
      Some(head) => {
        let commit = head.peel_to_commit()?;
        Some(commit.tree()?)
      }
      None => None,
    };

    // conflicted changes
    if rebase_dir.is_some() || is_merge_active(&repo) {
      let commit =
        get_current_commit(&repo)?.expect("There must be a current commit during a rebase");
      let tree = commit.tree()?;
      let diff = repo.diff_tree_to_index(Some(&tree), None, None)?;
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

    // staged changes
    let diff = repo
      .diff_tree_to_index(tree.as_ref(), None, None)
      .context("Failed to get staged changes")?;

    match DiffSummary::new(&diff) {
      Ok(it) => {
        // filter out conflicted files
        let it = it.non_conflicts();
        if it.num_files != 0 {
          println!();
          print!("{} - ", style("Staged").green());
          println!("{}", it)
        }
      }
      Err(_) => {
        println!();
        println!("Failed to get summary of staged changes");
      }
    };

    // unstaged changes
    let mut opts = if self.hide_untracked || cli.config.hide_untracked {
      None
    } else {
      let mut opts = DiffOptions::new();
      opts.include_untracked(true);
      Some(opts)
    };

    let diff = repo
      .diff_index_to_workdir(None, opts.as_mut())
      .context("Failed to get unstaged changes")?;

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

/// Header lines to display when there's no active state (rebase, etc.)
fn display_normal_header(repo: &Repository, head: Option<&Reference>) -> Result<String> {
  let mut out = String::new();
  let mut branch_name = None;

  let first_line = match head {
    // there are commits in the repo
    Some(head) => {
      let commit = head
        .peel_to_commit()
        .context("Failed to get commit at HEAD")?;

      // display branch name or detached head indicator
      let display_branch = if head.is_branch() {
        let name = lossy!(head.shorthand_bytes()).to_string();
        branch_name = Some(name.clone());
        format!("On {}", style(&name).green())
      } else {
        style("Detached HEAD").red().to_string()
      };

      format!(
        "{} -> {} {}",
        display_branch,
        display_hash(&commit.id()),
        lossy!(
          commit
            .summary_bytes()
            .context("Failed to get commit message")?
        )
      )
    }

    // head points to nothing, no commits in repo
    None => style("No commits yet").dim().to_string(),
  };

  // end first line
  if is_term() {
    out.push_str(&format!(
      "{}",
      truncate_str(
        &first_line,
        get_term_width(),
        &style("\u{2026}").dim().to_string()
      )
    ));
  } else {
    out.push_str(&first_line);
  }

  // upstream and base ahead/behind if we're on a branch
  if head.is_some_and(|it| it.is_branch()) {
    let branch_name = branch_name.context("Branch name should exist when HEAD is not detached")?;
    let branch = name_to_branch(repo, &branch_name)?;

    let mut rows: Vec<[String; 2]> = Vec::new();
    // the label is either "Upstream" or "Base", these are printed with alignment so the branch
    // names are lined up
    let mut label_width = 0usize;

    // upstream row
    let upstream = get_upstream(&branch)?;
    if let Some(upstream) = upstream {
      let upstream_name = branch_to_name(&upstream)?;
      let (a, b) = get_ahead_behind(repo, branch.get(), upstream.get())
        .context("Failed to get ahead/behind for upstream")?;

      let row = [
        style("Upstream").blue().to_string(),
        format!(
          "{}{} {}{}",
          style("[").dim(),
          style(&upstream_name),
          display_plus_minus(a, b),
          style("]").dim(),
        ),
      ];
      label_width = measure_text_width(&row[0]);
      rows.push(row);
    }

    // base row
    let base_name = data::get_feature_base(&data::git_config(repo)?, &branch_name);
    if let Some(base_name) = base_name {
      let (a, b) = get_ahead_behind(repo, branch.get(), &repo.find_reference(&base_name)?)
        .context("Failed to get ahead/behind for base")?;

      let row = [
        style("Base").magenta().to_string(),
        format!(
          "{}{} {}{}",
          style("[").dim(),
          style(
            &base_name
              .trim_prefix("refs/heads/")
              .trim_prefix("refs/remotes/")
          ),
          display_plus_minus(a, b),
          style("]").dim(),
        ),
      ];
      label_width = label_width.max(measure_text_width(&row[0]));
      rows.push(row);
    }

    // print with everything after the row label aligned
    for row in rows {
      out.push_str(&format!(
        "\n  {} {}",
        pad_str(&row[0], label_width, console::Alignment::Left, None),
        &row[1]
      ));
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

  let head_name = fs::read_to_string(dir.join("head-name")).context("Failed to get branch name")?;
  let branch = head_name.trim().trim_prefix("refs/heads/");

  let onto = fs::read_to_string(dir.join("onto")).context("Failed to get base commit")?;
  let onto = onto.trim();

  // this must be parseable as an id
  let base_id = Oid::from_str(onto)?;

  // try to find a matching branch, but don't error
  let base = match commit_to_branch(repo, &base_id) {
    Ok(branch) => match branch {
      Some(branch) => match branch.name_bytes() {
        Ok(name) => Some(lossy!(name).to_string()),
        Err(_) => None,
      },
      None => None,
    },
    Err(_) => None,
  };

  Ok(format!(
    "{} {} onto {} {}",
    style("Rebasing").yellow(),
    style(&branch).blue(),
    style(&base.unwrap_or(trim_hash(&base_id))).magenta(),
    progress
  ))
}

fn is_merge_active(repo: &Repository) -> bool {
  repo.path().join("MERGE_HEAD").exists()
}

/// Displays a summary of an ongoing merge
///
/// - `current` - a string representation of where the user is. This will usually be a branch, but
///   can be a hash in detached head state
fn display_merge_header(repo: &Repository) -> Result<String> {
  let merge_head = fs::read_to_string(repo.path().join("MERGE_HEAD"))?;
  let merge_head = merge_head.trim();
  let other_commit = Oid::from_str(merge_head)?;

  // current branch if it was detected, else current commit
  let current = get_current_branch_name(repo)?.unwrap_or(
    trim_hash(
      &get_current_commit(repo)?
        .expect("HEAD should point to a commit during an ongoing merge")
        .id(),
    )
    .to_string(),
  );

  // get the branch pointed to by MERGE_HEAD, else just use the hash
  let base = match commit_to_branch(repo, &other_commit)? {
    Some(branch) => match branch.name_bytes() {
      Ok(name) => lossy!(name).to_string(),
      Err(_) => "unknown".to_string(),
    },
    None => trim_hash(&other_commit),
  };

  Ok(format!(
    "{} {} with {}",
    style("Merging").yellow(),
    style(current).blue(),
    style(base).magenta()
  ))
}
