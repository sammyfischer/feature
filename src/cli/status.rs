use std::fmt::Write;
use std::path::Path;
use std::{fs, thread};

use anyhow::{Context, Result};
use console::{measure_text_width, pad_str, style, truncate_str};
use git2::{
  Commit,
  DiffOptions,
  ErrorClass,
  ErrorCode,
  Oid,
  Reference,
  Repository,
  RepositoryState,
};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::cli::display::commit::display_commit_compact;
use crate::cli::display::diff::{display_summary, display_summary_header};
use crate::cli::display::{display_plus_minus, display_signature};
use crate::core::advice::{
  BISECT_ADVICE,
  MERGE_CONFLICT_ADVICE,
  PICK_CONFLICT_ADVICE,
  REBASE_CONFLICT_ADVICE,
  REVERT_CONFLICT_ADVICE,
  STATUS_ADVICE,
};
use crate::core::branch::{
  get_ahead_behind,
  get_current_branch_or_commit,
  get_head,
  get_merge_head,
  get_pick_head,
  get_revert_head,
};
use crate::core::branch_info::BranchInfo;
use crate::core::commit::find_branch_at_commit;
use crate::core::diff::DiffSummary;
use crate::core::project_config::projects::ProjectEntry;
use crate::core::status::{
  get_conflicts,
  get_staged_changes,
  get_unstaged_changes,
  is_conflictable_active,
  is_pick_active,
};
use crate::core::string::{ToStrLossy, ToStrLossyOwned, TrimPrefix};
use crate::core::tag::find_current_semver;
use crate::core::term::{get_term_width, is_term};
use crate::core::user_config::UserConfig;
use crate::core::{get_signature, open_repo_from_dirs, trim_hash};
use crate::{App, opt_advice, style};

#[derive(clap::Args, Clone, Debug)]
#[command(
  visible_alias = "st",
  about = "View current status (current branch, author info, changes)",
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
    let repo_dir = state.repo.path().to_owned();
    let work_dir = state.repo.workdir().to_owned();
    let proj_config = &state.config;
    let user_config = UserConfig::new(&state.repo)?;

    let hide_projects = match self.no_projects {
      Some(it) => it,
      None => !user_config.show_projects()?,
    };

    let hide_modules = match self.no_modules {
      Some(it) => it,
      None => !user_config.show_modules()?,
    };
    let mod_names: Vec<_> = if hide_modules {
      Vec::new()
    } else {
      state
        .repo
        .submodules()?
        .iter()
        .map(|module| module.name_bytes().to_str_lossy_owned())
        .collect()
    };

    thread::scope(|scope| -> Result<_> {
      let repo_thread = scope.spawn(|| {
        let repo = open_repo_from_dirs(&repo_dir, work_dir)?;
        self.display_main_repo(&repo)
      });

      let proj_thread = scope.spawn(|| {
        if hide_projects {
          return Vec::new();
        }
        proj_config
          .projects
          .par_iter()
          .map(|project| -> Result<String> {
            let repo = open_repo_from_dirs(&repo_dir, work_dir)?;
            self.display_project(&repo, project)
          })
          .collect()
      });

      let mod_thread = scope.spawn(|| {
        if hide_modules {
          return Vec::new();
        }
        mod_names
          .par_iter()
          .map(|name| -> Result<String> {
            let repo = open_repo_from_dirs(&repo_dir, work_dir)?;
            self.display_module(&repo, name)
          })
          .collect()
      });

      let repo_result = repo_thread.join().unwrap();
      match repo_result {
        Ok(out) => println!("{}", out),
        Err(e) => eprintln!("{}", e),
      }

      let proj_results = proj_thread.join().unwrap();
      if !proj_results.is_empty() {
        println!("\n{}", style("Projects").bold().blue());
        for result in proj_results {
          match result {
            Ok(out) => println!("{}", out),
            Err(e) => eprintln!("{}", e),
          }
        }
      }

      let mod_results = mod_thread.join().unwrap();
      if !mod_results.is_empty() {
        println!("\n{}", style("Modules").bold().magenta());
        for result in mod_results {
          match result {
            Ok(out) => println!("{}", out),
            Err(e) => eprintln!("{}", e),
          }
        }
      }

      Ok(())
    })
  }

  /// Displays main repo status
  fn display_main_repo(&self, repo: &Repository) -> Result<String> {
    use std::fmt::Write;
    let mut out = String::new();
    let config = UserConfig::new(repo)?;
    let head = get_head(repo)?;

    let (header, advice) = match repo.state() {
      // TODO: custom header/advice for git am
      RepositoryState::ApplyMailbox | RepositoryState::Clean => (
        display_normal_header(repo, head.as_ref())?,
        opt_advice!(config.advice_status()?, STATUS_ADVICE),
      ),

      RepositoryState::Merge => (
        display_merge_header(repo)?,
        opt_advice!(config.advice_conflict()?, MERGE_CONFLICT_ADVICE),
      ),

      RepositoryState::Revert | RepositoryState::RevertSequence => (
        display_revert_header(repo)?,
        opt_advice!(config.advice_conflict()?, REVERT_CONFLICT_ADVICE),
      ),

      RepositoryState::CherryPick | RepositoryState::CherryPickSequence => (
        display_pick_header(repo)?,
        opt_advice!(config.advice_conflict()?, PICK_CONFLICT_ADVICE),
      ),

      RepositoryState::Bisect => (
        display_bisect_header(repo)?,
        opt_advice!(config.advice_status()?, BISECT_ADVICE),
      ),

      RepositoryState::Rebase
      | RepositoryState::RebaseInteractive
      | RepositoryState::RebaseMerge => (
        display_rebase_header(repo, &repo.path().join("rebase-merge"))?,
        opt_advice!(config.advice_conflict()?, REBASE_CONFLICT_ADVICE),
      ),

      RepositoryState::ApplyMailboxOrRebase => (
        display_rebase_header(repo, &repo.path().join("rebase-apply"))?,
        opt_advice!(config.advice_conflict()?, REBASE_CONFLICT_ADVICE),
      ),
    };

    write!(out, "{}", header)?;

    // print advice in new paragraph above diffs
    if let Some(advice) = advice {
      write!(out, "\n\n{}", advice)?;
    }

    if is_pick_active(repo) {
      // cherry picks are weird bc they show no diff with head when you stage changes.
      // to show meaningful changes you have to diff with the picked commit
      let pick_head = repo.find_reference("CHERRY_PICK_HEAD")?;
      let pick_tree = pick_head.peel_to_tree()?;

      let diff = repo.diff_tree_to_index(Some(&pick_tree), None, None)?;
      let summary = DiffSummary::new(&diff)?.non_conflicts();

      if summary.num_files != 0 {
        write!(
          out,
          "\n\n{} - {}",
          style("Resolved").green(),
          display_summary(&summary)
        )?;
      }

      // cherry picked changes have no difference with head (except for conflicts), so
      // the remaining diffs can be skipped
      return Ok(out);
    }

    let show_untracked = match self.no_untracked {
      Some(hide) => !hide,
      None => config.status_untracked()?,
    };

    let statuses = display_file_statuses(repo, show_untracked)?;
    if !statuses.is_empty() {
      write!(out, "\n\n{}", display_file_statuses(repo, show_untracked)?)?;
    }
    Ok(out)
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
    let config = UserConfig::new(repo)?;

    out.push_str(&style(name).cyan().to_string());

    let proj_repo = match Repository::open(&project.path) {
      Ok(it) => it,
      Err(e)
        if (e.class() == ErrorClass::Os || e.class() == ErrorClass::Repository)
          && e.code() == ErrorCode::NotFound =>
      {
        out.push_str(" not initialized");
        return Ok(out);
      }
      Err(e) => return Err(e.into()),
    };

    if let Some(head) = get_head(&proj_repo)? {
      let commit = head.peel_to_commit()?;

      if head.is_branch() || head.is_remote() {
        let branch_info = BranchInfo::from_reference(&head)?;
        out.push_str(&format!(" on {}", style(branch_info.name()).green()));

        if let Some(upstream) = branch_info.upstream(&proj_repo)? {
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

      if config.show_authorship()? {
        out.push_str(&self.display_different_signature(repo, &proj_repo)?);
      }
      out.push_str(&self.display_subrepo_changes(&proj_repo, &commit)?);
    };

    Ok(out)
  }

  /// Builds output for a particular submodule
  ///
  /// # Params
  /// - `repo` - The *parent* repo, not the submodule repo
  /// - `module` - The submodule, usually obtained from [Repository::submodules]
  fn display_module(&self, repo: &Repository, mod_name: &str) -> Result<String> {
    let mut out = String::new();
    out.push_str(&style(mod_name).cyan().to_string());

    let config = UserConfig::new(repo)?;
    let module = repo.find_submodule(mod_name)?;

    let mod_repo = match module.open() {
      Ok(it) => it,
      Err(e)
        if (e.class() == ErrorClass::Os || e.class() == ErrorClass::Repository)
          && e.code() == ErrorCode::NotFound =>
      {
        out.push_str(" not initialized");
        return Ok(out);
      }
      Err(e) => return Err(e.into()),
    };

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

      if config.show_authorship()? {
        out.push_str(&self.display_different_signature(repo, &mod_repo)?);
      }
      out.push_str(&self.display_subrepo_changes(&mod_repo, &commit)?);
    };

    Ok(out)
  }

  /// Displays appednable signature (tabbed on a new line) only if it differs
  /// from parent
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
      None => UserConfig::new(repo)?.status_untracked()?,
    };
    opts.include_untracked(include);
    let mut unstaged = repo.diff_index_to_workdir(None, Some(&mut opts))?;
    unstaged.find_similar(None)?;
    let unstaged = DiffSummary::new(&unstaged)?;

    if staged.num_files > 0 {
      out.push_str(&format!(
        "\n  {} - {}",
        style("Staged").green(),
        display_summary_header(&staged)
      ));
    }
    if unstaged.num_files > 0 {
      out.push_str(&format!(
        "\n  {} - {}",
        style("Unstaged").red(),
        display_summary_header(&unstaged)
      ));
    }

    Ok(out)
  }
}

/// Displays a header when there is no other active operation (e.g. rebase/merge
/// conflicts). Shows current branch, commit it points to, and upstream/base
/// info if available. Unlike the others, this header takes up to 3 lines.
fn display_normal_header(repo: &Repository, head: Option<&Reference>) -> Result<String> {
  let mut out = String::with_capacity(80);
  let mut branch = None;

  match head {
    // there are commits in the repo
    Some(head) => {
      let commit = head
        .peel_to_commit()
        .context("Failed to get commit at HEAD")?;

      // display branch name or detached head indicator
      let display_branch = if head.is_branch() {
        let info = BranchInfo::from_reference(head)?;
        let mut out = format!("On {}", style(info.name()).green());

        let semver = find_current_semver(repo, &head.peel_to_commit()?)?;
        if let Some(semver) = semver {
          out.push_str(&format!(" {}", style!("({})", semver).dim()));
        }

        branch = Some(info);
        out
      } else if head.is_remote() {
        let meta = BranchInfo::from_reference(head)?;
        // don't set branch because, while remotes are like branches, they cannot have
        // an upstream or base
        format!("On remote {}", style(meta.name()).green())
      } else if head.is_tag() {
        let name = head.name_bytes().to_str_lossy();
        let mut out = format!("On tag {}", style(&name).green());

        let semver = find_current_semver(repo, &head.peel_to_commit()?)?;
        if let Some(semver) = semver {
          // display semver if it's a different tag
          if semver.name() != name {
            out.push_str(&format!(" {}", style!("({})", semver).dim()));
          }
        }

        out
      } else {
        style("Detached HEAD").red().to_string()
      };

      write!(out, "{}", display_branch)?;

      display_authorship(repo, &mut out, " as ")?;

      let commit = display_commit_compact(&commit)?;
      let commit = if is_term() {
        truncate_str(&commit, get_term_width(), &style("…").dim().to_string()).to_string()
      } else {
        commit
      };
      write!(out, "\n{}", commit)?;
    }

    // head points to nothing, no commits in repo
    None => {
      let head = repo.find_reference("HEAD")?;
      let symbolic_ref = head
        .symbolic_target_bytes()
        .expect("HEAD points to nothing. Is the .git/HEAD file corrupt or missing?")
        .to_str_lossy();

      write!(
        out,
        "On {} (no commits yet)",
        style(symbolic_ref.trim_prefix_opt("refs/heads/")).green()
      )?;

      display_authorship(repo, &mut out, "\n  Committing as ")?;
    }
  };

  // upstream and base ahead/behind if we're on a branch
  if let Some(branch) = branch {
    // we don't fetch, so we can use this ref multiple times
    let branch_ref = branch.resolve(repo)?;

    let mut rows: Vec<[String; 2]> = Vec::with_capacity(2);
    // the label is either "Upstream" or "Base", these are printed with alignment so
    // the branch names are lined up
    let mut label_width = 0usize;

    // upstream row
    let upstream = branch.upstream(repo)?;
    if let Some(upstream) = upstream {
      let upstream = BranchInfo::from_branch(&upstream)?;
      let (a, b) = get_ahead_behind(repo, &upstream.resolve(repo)?, &branch_ref)
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
    let base = UserConfig::new(repo)?.branch_base(branch.name())?;
    if let Some(base) = base {
      let (a, b) = get_ahead_behind(repo, &base.resolve(repo)?, &branch_ref)
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

/// Displays a header line for an active rebase. Includes the source and
/// destination branches, and the current progress.
fn display_rebase_header(repo: &Repository, dir: &Path) -> Result<String> {
  let msgnum =
    fs::read_to_string(dir.join("msgnum")).context("Failed to get current step number")?;
  let current = msgnum.trim();

  let end = fs::read_to_string(dir.join("end")).context("Failed to get total number of steps")?;
  let total = end.trim();

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
  .unwrap_or(trim_hash(base_commit.as_object())?);

  let mut out = String::with_capacity(80);
  write!(
    out,
    "{} {} onto {} {}",
    style("Rebasing").yellow(),
    style(&branch_name).blue(),
    style(&base).magenta(),
    style!("({}/{})", current, total).dim()
  )?;

  display_authorship(repo, &mut out, " as ")?;
  Ok(out)
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
    None => trim_hash(merge_commit.as_object())?,
  };

  let mut out = String::with_capacity(80);
  write!(
    out,
    "{} {} with {}",
    style("Merging").yellow(),
    style(current).blue(),
    style(base).magenta()
  )?;

  display_authorship(repo, &mut out, " as ")?;
  Ok(out)
}

/// Displays a header line for an active cherry-pick conflict
fn display_pick_header(repo: &Repository) -> Result<String> {
  let pick_head = get_pick_head(repo)?.context("Reference CHERRY_PICK_HEAD does not exist")?;
  let pick_commit = pick_head.peel_to_commit()?;

  let current = get_current_branch_or_commit(repo)?
    .expect("HEAD should point to a commit during an active cherry-pick");

  let mut out = String::with_capacity(80);
  write!(
    out,
    "{} {} onto {}",
    style("Picking").yellow(),
    style(trim_hash(pick_commit.as_object())?).blue(),
    style(current).magenta()
  )?;

  display_authorship(repo, &mut out, " as ")?;
  Ok(out)
}

fn display_revert_header(repo: &Repository) -> Result<String> {
  let revert_head = get_revert_head(repo)?.context("Reference REVERT_HEAD does not exist")?;
  let revert_commit = revert_head.peel_to_commit()?;

  // current branch if it was detected, else current commit
  let current = get_current_branch_or_commit(repo)?
    .expect("HEAD should point to a commit during an active revert");

  let mut out = String::with_capacity(80);
  write!(
    out,
    "{} changes from {} onto {}",
    style("Reverting").yellow(),
    style(trim_hash(revert_commit.as_object())?).blue(),
    style(current).magenta()
  )?;

  display_authorship(repo, &mut out, " as ")?;
  Ok(out)
}

fn display_bisect_header(repo: &Repository) -> Result<String> {
  let current = get_current_branch_or_commit(repo)?
    .expect("HEAD should point to a commit during an active bisect");

  let start_path = repo.path().join("BISECT_START");
  let mut start = fs::read_to_string(&start_path)?.trim().to_string();

  if let Ok(id) = Oid::from_str(&start) {
    let commit = repo.find_commit(id)?;
    start = trim_hash(commit.as_object())?;
  }

  let mut out = String::with_capacity(80);
  write!(
    out,
    "{} on {} {}",
    style("Bisecting").yellow(),
    style(&current).blue(),
    style(&format!("(started from {})", start)).dim()
  )?;

  display_authorship(repo, &mut out, "\n")?;
  Ok(out)
}

/// Writes the prefix and authorship info to the buffer if the user's config
/// allows it. The prefix immediately precedes the authorship info with no added
/// whitespace.
fn display_authorship<'buf>(
  repo: &Repository,
  buf: &'buf mut String,
  prefix: &str,
) -> Result<&'buf mut String> {
  let config = UserConfig::new(repo)?;
  if config.show_authorship()? {
    write!(
      buf,
      "{}{}",
      prefix,
      display_signature(get_signature(repo)?.as_ref())
    )?;
  }

  Ok(buf)
}

/// Gets conflicted, staged, and unstaged changes, and builds a printable
/// output.
///
/// # Params
/// - `untracked` - whether to include untracked files in the unstaged section
pub fn display_file_statuses(repo: &Repository, untracked: bool) -> Result<String> {
  use std::fmt::Write;
  let mut out = String::new();
  let mut first_paragraph = true;

  let conflicts = get_conflicts(repo)?;
  if !conflicts.is_empty() {
    first_paragraph = false;

    write!(
      out,
      "{} - {} files",
      style("Conflicts").yellow(),
      style(conflicts.len()).cyan()
    )?;

    for conflict in conflicts {
      write!(out, "\n  {}", conflict)?;
    }
  } else if is_conflictable_active(repo) {
    // state that could have conflicts, but there are currently no conflicts
    write!(
      out,
      "{} - {}",
      style("Conflicts").yellow(),
      style("none").green()
    )?;
  }

  let staged = get_staged_changes(repo)?;
  if staged.num_files != 0 {
    if first_paragraph {
      first_paragraph = false;
    } else {
      write!(out, "\n\n")?;
    }
    write!(
      out,
      "{} - {}",
      style("Staged").green(),
      display_summary(&staged)
    )?;
  }

  let unstaged = get_unstaged_changes(repo, untracked)?;
  if unstaged.num_files != 0 {
    if !first_paragraph {
      write!(out, "\n\n")?;
    }
    write!(
      out,
      "{} - {}",
      style("Unstaged").red(),
      display_summary(&unstaged)
    )?;
  }

  Ok(out)
}

/// Guide for what each letter means
pub fn status_guide() -> String {
  use std::fmt::Write;
  let mut out = String::with_capacity(400);

  writeln!(out, "Meaning of each file status").unwrap();
  writeln!(out, "  {} Added", style("A").green()).unwrap();
  writeln!(out, "  {} Deleted", style("D").red()).unwrap();
  writeln!(out, "  {} Modified", style("M").yellow()).unwrap();
  writeln!(out, "  {} Untracked", style("U").cyan()).unwrap();
  writeln!(out, "  {} Conflicted", style("X").red()).unwrap();

  writeln!(out, "These display the old and new name").unwrap();
  writeln!(out, "  {} Renamed", style("R").magenta()).unwrap();
  writeln!(out, "  {} Copied", style("C").magenta()).unwrap();

  writeln!(out, "These appear under conflicts").unwrap();
  writeln!(out, "  {} None (file not present)", style("-").dim()).unwrap();

  writeln!(out, "These generally won't appear in regular statuses").unwrap();
  writeln!(out, "  {} Unmodified", style("=").dim()).unwrap();
  writeln!(out, "  {} Ignored", style("I").dim()).unwrap();
  writeln!(out, "  {} Typechange", style("T").yellow()).unwrap();
  writeln!(out, "  {} Unreadable", style("?").red()).unwrap();
  out
}
