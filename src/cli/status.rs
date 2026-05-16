use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use console::{measure_text_width, pad_str, style, truncate_str};
use git2::{Commit, DiffOptions, Oid, Reference, Repository, Submodule};

use crate::config::projects::ProjectEntry;
use crate::util::advice::{
  BISECT_ADVICE, MERGE_CONFLICT_ADVICE, PICK_CONFLICT_ADVICE, REBASE_CONFLICT_ADVICE,
  REVERT_CONFLICT_ADVICE, STATUS_ADVICE,
};
use crate::util::branch::{
  find_branch_at_commit, get_ahead_behind, get_current_branch_or_commit, get_head, get_merge_head,
  get_pick_head, get_revert_head,
};
use crate::util::branch_meta::BranchMeta;
use crate::util::diff::DiffSummary;
use crate::util::display::{
  display_commit_compact, display_plus_minus, display_signature, trim_hash,
};
use crate::util::get_signature;
use crate::util::string::{ToStrLossy, ToStrLossyOwned, TrimPrefix};
use crate::util::term::{get_term_width, is_term};
use crate::{App, data, opt_advice};

#[derive(clap::Args, Clone, Debug)]
#[command(
  visible_alias = "st",
  about = "View current status (current branch, author info, changes)",
  disable_help_flag = true,
  disable_help_subcommand = true
)]
pub struct Args {
  /// Hides untracked files from output
  #[arg(short = 'U', long, value_name = "HIDE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  pub no_untracked: Option<bool>,

  /// Hides feature subprojects from output
  #[arg(short = 'P', long, value_name = "HIDE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  pub no_projects: Option<bool>,

  /// Hides git submodules from output
  #[arg(short = 'M', long, value_name = "HIDE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  pub no_modules: Option<bool>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let config = state.repo.config()?.snapshot()?;
    let head = get_head(&state.repo)?;
    let rebase_dir = get_rebase_dir(&state.repo);

    let (header, advice) = if let Some(dir) = rebase_dir.as_ref() {
      (
        display_rebase_header(&state.repo, dir)?,
        opt_advice!(data::get_advice_conflict(&config)?, REBASE_CONFLICT_ADVICE),
      )
    } else if is_merge_active(&state.repo) {
      (
        display_merge_header(&state.repo)?,
        opt_advice!(data::get_advice_conflict(&config)?, MERGE_CONFLICT_ADVICE),
      )
    } else if is_pick_active(&state.repo) {
      (
        display_pick_header(&state.repo)?,
        opt_advice!(data::get_advice_conflict(&config)?, PICK_CONFLICT_ADVICE),
      )
    } else if is_revert_active(&state.repo) {
      (
        display_revert_header(&state.repo)?,
        opt_advice!(data::get_advice_conflict(&config)?, REVERT_CONFLICT_ADVICE),
      )
    } else if is_bisect_active(&state.repo) {
      (
        display_bisect_header(&state.repo)?,
        opt_advice!(data::get_advice_status(&config)?, BISECT_ADVICE),
      )
    } else {
      (
        display_normal_header(state, head.as_ref())?,
        opt_advice!(data::get_advice_status(&config)?, STATUS_ADVICE),
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

    let hide_untracked = match self.no_untracked {
      Some(it) => it,
      None => !data::get_status_untracked(&config)?,
    };

    let mut opts = if hide_untracked {
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

    let hide_projects = match self.no_projects {
      Some(it) => it,
      None => !data::get_feature_show_projects(&config)?,
    };

    if !hide_projects {
      let projects = &state.config.projects;
      if !projects.is_empty() {
        println!("\n{}", style("Projects").bold().blue());
      }
      for project in &state.config.projects {
        println!("{}", self.display_project(&state.repo, project)?);
      }
    }

    let hide_modules = match self.no_modules {
      Some(it) => it,
      None => !data::get_feature_show_modules(&config)?,
    };

    if !hide_modules {
      let modules = state.repo.submodules()?;
      if !modules.is_empty() {
        println!("\n{}", style("Modules").bold().magenta());
      }
      for module in modules {
        println!("{}", self.display_module(&state.repo, &module)?);
      }
    }

    Ok(())
  }

  /// Builds output for a particular subproject
  ///
  /// # Params
  /// - `repo` - The *parent* repo, not the subproject repo
  /// - `project` - A tuple of the project name and [ProjectEntry]
  fn display_project(
    &self,
    repo: &Repository,
    project: (&String, &ProjectEntry),
  ) -> Result<String> {
    let mut out = String::new();
    let (name, project) = project;

    out.push_str(&style(name).cyan().to_string());

    let proj_repo = Repository::open(&project.path)?;

    if let Some(head) = get_head(&proj_repo)? {
      let commit = head.peel_to_commit()?;

      if head.is_branch() || head.is_remote() {
        let branch_meta = BranchMeta::from_reference(&head)?;
        out.push_str(&format!(" on {}", style(branch_meta.name()).green()));

        if let Some(upstream) = branch_meta.upstream(&proj_repo)? {
          let (ahead, behind) = get_ahead_behind(&proj_repo, &head, upstream.get())?;
          out.push_str(&format!(
            " {}{}{}",
            style("[").dim(),
            display_plus_minus(ahead, behind),
            style("]").dim()
          ));
        }
      } else if head.is_tag() {
        let name = head.shorthand_bytes().to_str_lossy();
        out.push_str(&format!(" on {}", style(name).green()));
      }
      out.push_str(&format!(" -> {}", display_commit_compact(&commit)?));

      if is_term() {
        out = truncate_str(&out, get_term_width(), "\u{2026}").to_string();
      }

      out.push_str(&self.display_different_signature(repo, &proj_repo)?);
      out.push_str(&self.display_subrepo_changes(&proj_repo, &commit)?);
    };

    Ok(out)
  }

  /// Builds output for a particular submodule
  ///
  /// # Params
  /// - `repo` - The *parent* repo, not the submodule repo
  /// - `module` - The submodule, usually obtained from [Repository::submodules]
  fn display_module(&self, repo: &Repository, module: &Submodule) -> Result<String> {
    let mut out = String::new();

    let name = module.name_bytes().to_str_lossy();
    out.push_str(&style(name).cyan().to_string());

    let mod_repo = module.open()?;

    // committed state of submodule (commit parent expects module to be on)
    let head_id = module.head_id();
    // current state of submodule (commit module is actually on)
    let index_id = module.index_id();

    match (index_id, head_id) {
      (Some(index_id), Some(head_id)) => {
        let (ahead, behind) = mod_repo.graph_ahead_behind(index_id, head_id)?;
        out.push_str(&format!(
          " {}{}{}",
          style("[").dim(),
          display_plus_minus(ahead, behind),
          style("]").dim()
        ));
      }

      (Some(_), None) => {
        out.push_str(&format!(
          " {}{}{}",
          style("[").dim(),
          style("untracked").red(),
          style("]").dim()
        ));
      }
      _ => (),
    }

    // actual repo info
    if let Some(head) = get_head(&mod_repo)? {
      let commit = head.peel_to_commit()?;

      if head.is_branch() || head.is_remote() || head.is_tag() {
        let name = head.shorthand_bytes().to_str_lossy();
        out.push_str(&format!(" on {}", style(name).green()));
      }
      out.push_str(&format!(" -> {}", display_commit_compact(&commit)?));

      if is_term() {
        out = truncate_str(&out, get_term_width(), "\u{2026}").to_string();
      }

      out.push_str(&self.display_different_signature(repo, &mod_repo)?);
      out.push_str(&self.display_subrepo_changes(&mod_repo, &commit)?);
    };

    Ok(out)
  }

  /// Displays appednable signature (tabbed on a new line) only if it differs from parent
  fn display_different_signature(&self, parent: &Repository, child: &Repository) -> Result<String> {
    let mut out = String::new();
    let parent_sig = get_signature(parent)?;
    let child_sig = get_signature(child)?;

    if let Some(child_sig) = child_sig {
      if let Some(parent_sig) = parent_sig
        && parent_sig.name_bytes() == child_sig.name_bytes()
        && parent_sig.email_bytes() == child_sig.email_bytes()
      {
        // name and email are the same, don't display signature
      } else {
        out.push_str(&format!("\n  {}", display_signature(Some(&child_sig))));
      }
    } else {
      // default text for when no name/email is found
      out.push_str(&format!("\n  {}", display_signature(None)));
    }

    Ok(out)
  }

  /// Displays appendable staged and unstaged changes tabbed on a new line
  fn display_subrepo_changes(&self, repo: &Repository, head: &Commit) -> Result<String> {
    let mut out = String::new();

    let tree = head.tree()?;
    let mut staged = repo.diff_tree_to_index(Some(&tree), None, None)?;
    staged.find_similar(None)?;
    let staged = DiffSummary::new(&staged)?;

    let mut opts = DiffOptions::new();
    let include = match self.no_untracked {
      Some(it) => !it,
      None => data::get_status_untracked(&repo.config()?.snapshot()?)?,
    };
    opts.include_untracked(include);
    let mut unstaged = repo.diff_index_to_workdir(None, Some(&mut opts))?;
    unstaged.find_similar(None)?;
    let unstaged = DiffSummary::new(&unstaged)?;

    if staged.num_files > 0 {
      out.push_str(&format!(
        "\n  {} - {}",
        style("Staged").green(),
        staged.display_header()
      ));
    }
    if unstaged.num_files > 0 {
      out.push_str(&format!(
        "\n  {} - {}",
        style("Unstaged").red(),
        unstaged.display_header()
      ));
    }

    Ok(out)
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
        style(symbolic_ref.trim_prefix_opt("refs/heads/")).green()
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

  // 'onto' must be parseable as an id
  let base_commit = repo.find_commit(Oid::from_str(onto).with_context(|| {
    format!(
      "{} should contain a valid commit hash",
      onto_path.to_string_lossy()
    )
  })?)?;

  // try to find a matching branch, but don't error
  let base = match find_branch_at_commit(repo, &base_commit.id()) {
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
  .unwrap_or(trim_hash(&base_commit)?);

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
  let base = match find_branch_at_commit(repo, &merge_commit.id())? {
    Some(branch) => match branch.name_bytes() {
      Ok(name) => name.to_str_lossy_owned(),
      Err(_) => "unknown".to_string(),
    },
    None => trim_hash(&merge_commit)?,
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
    style(trim_hash(&pick_commit)?).blue(),
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
    style(trim_hash(&revert_commit)?).blue(),
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
    start = trim_hash(&commit)?;
  }

  Ok(format!(
    "{} on {} {}",
    style("Bisecting").yellow(),
    style(&current).blue(),
    style(&format!("(started from {})", start)).dim()
  ))
}
