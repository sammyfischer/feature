use std::fmt::Write;
use std::path::Path;
use std::{fs, thread};

use anyhow::{Context, Result};
use console::{style, truncate_str};
use git2::{Commit, DiffOptions, Oid, Repository, RepositoryState};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::cli::advice::{
  BISECT_ADVICE,
  MERGE_CONFLICT_ADVICE,
  PICK_CONFLICT_ADVICE,
  REBASE_CONFLICT_ADVICE,
  REVERT_CONFLICT_ADVICE,
  STATUS_ADVICE,
};
use crate::cli::display::commit::display_commit_compact;
use crate::cli::display::diff::{display_summary, display_summary_header};
use crate::cli::display::time::{DisplayTimeOptions, display_time};
use crate::cli::display::{display_hash, display_plus_minus, display_signature};
use crate::cli::term::{get_term_width, is_term};
use crate::core::branch::{
  find_local_of_upstream,
  get_current_branch_or_commit,
  get_head,
  get_head_resolved,
  get_merge_head,
  get_pick_head,
  get_revert_head,
};
use crate::core::branch_info::BranchInfo;
use crate::core::commit::find_branch_at_commit;
use crate::core::diff::DiffSummary;
use crate::core::project_config::projects::ProjectEntry;
use crate::core::semver::{SemverTag, find_current_semver, since_prev_semver};
use crate::core::status::{
  get_conflicts,
  get_staged_changes,
  get_unstaged_changes,
  is_conflictable_active,
  is_pick_active,
};
use crate::core::string::{ToStrLossy, ToStrLossyOwned, TrimPrefix};
use crate::core::user_config::UserConfig;
use crate::core::wip::WipList;
use crate::core::{NotFoundExt, open_repo_from_dirs, trim_hash};
use crate::{App, dim_brackets, if_nerdfont, opt_advice, style};

const LONG_ABOUT: &str = r#"View repo status.

Shows useful info about where you're currently checked out, what changes are in
the workdir, what state the repo is in (e.g. merge conflict), and your current
authorship info (who you're committing as)."#;

#[derive(clap::Args, Clone, Debug)]
#[command(
  visible_alias = "st",
  about = "View repo status",
  long_about = LONG_ABOUT,
  disable_help_subcommand = true
)]
pub struct StatusArgs {
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

impl StatusArgs {
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

    let (header, advice) = match repo.state() {
      // TODO: custom header/advice for git am
      RepositoryState::ApplyMailbox | RepositoryState::Clean => (
        display_normal_header(repo, &config)?,
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

    let nerdfont = config.nerdfont()?;

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
          display_summary(&summary, nerdfont)
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

    let statuses = display_file_statuses(repo, show_untracked, nerdfont)?;
    if !statuses.is_empty() {
      write!(
        out,
        "\n\n{}",
        display_file_statuses(repo, show_untracked, nerdfont)?
      )?;
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

    let proj_repo = match Repository::open(&project.path).repo_not_found_ok()? {
      Some(it) => it,
      None => {
        out.push_str(" not initialized");
        return Ok(out);
      }
    };

    if let Some(head) = get_head_resolved(&proj_repo)? {
      let commit = head.peel_to_commit()?;

      if head.is_branch() || head.is_remote() {
        let branch_info = BranchInfo::from_reference(&head)?;
        out.push_str(&format!(" on {}", style(branch_info.name()).green()));

        if let Some(upstream) = branch_info.upstream(&proj_repo)? {
          let upstream_tip = upstream.get().peel_to_commit()?.id();
          let (ahead, behind) = proj_repo.graph_ahead_behind(commit.id(), upstream_tip)?;

          out.push(' ');
          out.push_str(dim_brackets!("{}", display_plus_minus(ahead, behind)));
        }
      } else if head.is_tag() {
        let name = head.shorthand_bytes().to_str_lossy();
        out.push_str(&format!(" on {}", style(name).green()));
      }
      out.push_str(&format!(
        " -> {}",
        display_commit_compact(&commit, &config, true)?
      ));

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

    let mod_repo = match module.open().repo_not_found_ok()? {
      Some(it) => it,
      None => {
        out.push_str(" not initialized");
        return Ok(out);
      }
    };

    // committed state of submodule (commit parent expects module to be on)
    let head_id = module.head_id();
    // current state of submodule (commit module is actually on)
    let index_id = module.index_id();

    match (index_id, head_id) {
      (Some(index_id), Some(head_id)) => {
        let (ahead, behind) = mod_repo.graph_ahead_behind(index_id, head_id)?;
        out.push(' ');
        out.push_str(dim_brackets!("{}", display_plus_minus(ahead, behind)));
      }

      (Some(_), None) => {
        out.push(' ');
        out.push_str(dim_brackets!("{}", style("untracked").red()));
      }
      _ => (),
    }

    // actual repo info
    if let Some(head) = get_head_resolved(&mod_repo)? {
      let commit = head.peel_to_commit()?;

      if head.is_branch() || head.is_remote() || head.is_tag() {
        let name = head.shorthand_bytes().to_str_lossy();
        out.push_str(&format!(" on {}", style(name).green()));
      }
      out.push_str(&format!(
        " -> {}",
        display_commit_compact(&commit, &config, true)?
      ));

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
    let parent_sig = parent.signature().not_found_ok()?;
    let child_sig = child.signature().not_found_ok()?;

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
fn display_normal_header(repo: &Repository, user_config: &UserConfig) -> Result<String> {
  let mut out = String::with_capacity(80);
  let nerdfont = user_config.nerdfont()?;

  match get_head_resolved(repo)? {
    Some(head) => {
      if head.is_branch() {
        // local branch
        let branch = BranchInfo::from_reference(&head)?;
        let commit = head.peel_to_commit()?;

        write!(out, "On branch {}", style(branch.name()).cyan())?;

        // small bites of info related to the branch
        //
        // displayed as:
        // [up: +0 -0 | base: +0 -0 | ver: 1.0.0 +13 | wips: 2]
        //
        // or with nerdfont:
        // [ +0 -0 |  +0 -0 |  1.0.0 +13 | 󱉚 2]
        let mut extras = Vec::new();

        if let Some(upstream) = branch.upstream(repo)? {
          let upstream_tip = upstream.get().peel_to_commit()?.id();
          let (a, b) = repo.graph_ahead_behind(upstream_tip, commit.id())?;

          extras.push(format!(
            "{} {}",
            style(if_nerdfont!(nerdfont, "", "up:")).blue(),
            display_plus_minus(a, b)
          ));
        }

        if let Some(base) = user_config.branch_base(branch.name())? {
          let base_tip = base.resolve(repo)?.peel_to_commit()?.id();
          let (a, b) = repo.graph_ahead_behind(base_tip, commit.id())?;

          extras.push(format!(
            "{} {}",
            style(if_nerdfont!(nerdfont, "", "base:")).magenta(),
            display_plus_minus(a, b)
          ));
        }

        if let Some(semver) = find_current_semver(repo, commit.id())? {
          let (since, _) = repo.graph_ahead_behind(commit.id(), semver.commit)?;

          extras.push(format!(
            "{} {}",
            style!(
              "{} {}",
              if_nerdfont!(nerdfont, "", "ver:"),
              &semver.name()[1..]
            )
            .yellow(),
            style!("+{}", since).green(),
          ));
        }

        let wips = WipList::from_branch(repo, branch.name().to_string())?;
        if !wips.is_empty() {
          extras.push(
            style!("{} {}", if_nerdfont!(nerdfont, "󱉚", "wips:"), wips.len())
              .cyan()
              .to_string(),
          );
        }

        if !extras.is_empty() {
          write!(
            out,
            " {}",
            dim_brackets!("{}", extras.join(&style(" | ").dim().to_string()))
          )?;
        }

        let commit_line = display_commit_compact(&commit, user_config, true)?;

        if is_term() {
          write!(
            out,
            "\n{}",
            truncate_str(
              &commit_line,
              get_term_width(),
              &style("\u{2026}").dim().to_string()
            )
          )?;
        } else {
          write!(out, "\n{}", &commit_line)?;
        }
      } else if head.is_remote() {
        // remote/upstream branch
        let upstream = BranchInfo::from_reference(&head)?;
        let commit = head.peel_to_commit()?;

        write!(out, "On upstream {}", style(upstream.name()).green())?;

        let mut info = Vec::new();

        if let Some(branch) = find_local_of_upstream(repo, &upstream)? {
          let branch_tip = branch.get().peel_to_commit()?.id();
          let (a, b) = repo.graph_ahead_behind(commit.id(), branch_tip)?;

          // doesn't need a prefix, this could only be relative to the local branch
          info.push(display_plus_minus(a, b));
        }

        if let Some(semver) = find_current_semver(repo, commit.id())? {
          info.push(
            style!("{} {}", if_nerdfont!(nerdfont, "", "ver:"), semver.name())
              .yellow()
              .to_string(),
          );
        }

        if !info.is_empty() {
          write!(
            out,
            " {}",
            dim_brackets!("{}", info.join(&style(" | ").dim().to_string()))
          )?;
        }

        let commit_line = display_commit_compact(&commit, user_config, true)?;

        if is_term() {
          write!(
            out,
            "\n{}",
            truncate_str(
              &commit_line,
              get_term_width(),
              &style("\u{2026}").dim().to_string()
            )
          )?;
        } else {
          write!(out, "\n{}", &commit_line)?;
        }
      } else if head.is_tag() {
        let tag_name = head.shorthand()?;
        let commit = head.peel_to_commit()?;

        write!(out, "On tag {}", style(tag_name).green())?;

        if let Ok(semver) = SemverTag::parse(tag_name) {
          let semver = SemverTag::from_tuple(semver, commit.id())?;
          if let Some((prev, since)) = since_prev_semver(repo, &semver)? {
            write!(
              out,
              " {}{} since {}{}",
              style("(").dim(),
              style!("+{}", since).green(),
              style(prev.name()).yellow(),
              style(")").dim(),
            )?;
          }
        }

        let commit_line = display_commit_compact(&commit, user_config, true)?;

        if is_term() {
          write!(
            out,
            "\n{}",
            truncate_str(
              &commit_line,
              get_term_width(),
              &style("\u{2026}").dim().to_string()
            )
          )?;
        } else {
          write!(out, "\n{}", &commit_line)?;
        }
      } else {
        // commit
        let commit = head.peel_to_commit()?;

        write!(
          out,
          "On commit {} {}",
          display_hash(commit.as_object())?,
          style!(
            "{}, {} · {}",
            commit.author().name()?,
            display_time(&commit.time(), &DisplayTimeOptions {
              relative: user_config.format_relative()?,
              fmt: user_config.format_date()?
            })?,
            commit.summary()?.unwrap_or(commit.message()?)
          )
          .dim()
        )?;
      }
    }

    // no commits yet
    None => {
      let head = get_head(repo)?.context("HEAD reference does not exist!")?;
      let branch_name = head
        .symbolic_target()?
        .context("Failed to get branch pointed to by HEAD")?
        .trim_prefix_opt("refs/heads/");

      write!(out, "On branch {}", style(branch_name).green())?;
      write!(
        out,
        "\n{}",
        style!("{}No commits yet", if_nerdfont!(nerdfont, " ")).dim()
      )?;
    }
  }

  if user_config.show_authorship()? {
    let sig = repo.signature()?;
    write!(
      out,
      "\n{} {}",
      style!("{}{}", if_nerdfont!(nerdfont, " "), sig.name()?).cyan(),
      style(sig.email()?).dim()
    )?;
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
    "{} {} into {}",
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
fn display_authorship(repo: &Repository, buf: &mut String, prefix: &str) -> Result<()> {
  let config = UserConfig::new(repo)?;
  if config.show_authorship()? {
    write!(
      buf,
      "{}{}",
      prefix,
      display_signature(repo.signature().not_found_ok()?.as_ref())
    )?;
  }

  Ok(())
}

/// Gets conflicted, staged, and unstaged changes, and builds a printable
/// output.
///
/// # Params
/// - `untracked` - whether to include untracked files in the unstaged section
/// - `nerdfont` - whether to use nerd font icons or regular characters
pub fn display_file_statuses(repo: &Repository, untracked: bool, nerdfont: bool) -> Result<String> {
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
      display_summary(&staged, nerdfont)
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
      display_summary(&unstaged, nerdfont)
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
