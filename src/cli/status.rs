use std::fmt::Write;
use std::{fs, thread};

use anyhow::{Context, Result};
use console::{style, truncate_str};
use git2::{Oid, Repository};
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
use crate::cli::display::diff::display_summary;
use crate::cli::display::time::{DisplayTimeOptions, display_time};
use crate::cli::display::{display_hash, display_plus_minus, display_signature};
use crate::cli::term::{get_term_width, is_term, trunc_term_width};
use crate::core::branch::{
  find_local_of_upstream,
  get_current_branch_or_commit,
  get_head_resolved,
  get_merge_head,
  get_pick_head,
  get_revert_head,
};
use crate::core::branch_info::BranchInfo;
use crate::core::commit::find_branch_at_commit;
use crate::core::diff::DiffSummary;
use crate::core::project_config::ProjectConfig;
use crate::core::project_config::projects::ProjectEntry;
use crate::core::rebase::RebaseInfo;
use crate::core::status::{
  CheckoutStatus,
  Conflict,
  StatusKind,
  get_conflicts,
  get_staged_changes,
  get_unstaged_changes,
  has_workdir_changes,
  is_conflictable_active,
};
use crate::core::string::{ToStrLossy, ToStrLossyOwned, TrimPrefix};
use crate::core::threading::ThreadedRepoHandle;
use crate::core::user_config::UserConfig;
use crate::core::version::{VersionTag, find_current_version, is_version_tag, since_prev_version};
use crate::core::wip::WipList;
use crate::core::{NotFoundExt, trim_hash};
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
    let handle = ThreadedRepoHandle::from(&state.repo);

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
        let repo = handle.open()?;
        self.display_main_repo(&repo, proj_config)
      });

      let proj_thread = scope.spawn(|| {
        if hide_projects {
          return Vec::new();
        }
        proj_config
          .projects
          .par_iter()
          .map(|project| -> Result<String> {
            let repo = handle.open()?;
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
            let repo = handle.open()?;
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
  fn display_main_repo(&self, repo: &Repository, proj_config: &ProjectConfig) -> Result<String> {
    use std::fmt::Write;
    let mut out = String::new();

    let user_config = UserConfig::new(repo)?;
    let nerdfont = user_config.nerdfont()?;

    // StatusKind tells us how to display the status, and what additional info
    // to get. It affects practically all subsequent info, so needs to be
    // calculated first.
    let kind = StatusKind::get(repo)?;

    // TODO: run this and file statuses in parallel
    let (header, advice) = match &kind {
      // TODO: custom header/advice for git am
      StatusKind::Clean { head, refname } => (
        display_normal_header(repo, proj_config, &user_config, head, refname)?,
        opt_advice!(user_config.advice_status()?, STATUS_ADVICE),
      ),

      StatusKind::Rebase(info) => (
        display_rebase_header(repo, info)?,
        opt_advice!(user_config.advice_conflict()?, REBASE_CONFLICT_ADVICE),
      ),

      StatusKind::Merge => (
        display_merge_header(repo)?,
        opt_advice!(user_config.advice_conflict()?, MERGE_CONFLICT_ADVICE),
      ),

      StatusKind::Pick => (
        display_pick_header(repo)?,
        opt_advice!(user_config.advice_conflict()?, PICK_CONFLICT_ADVICE),
      ),

      StatusKind::Revert => (
        display_revert_header(repo)?,
        opt_advice!(user_config.advice_conflict()?, REVERT_CONFLICT_ADVICE),
      ),

      StatusKind::Bisect => (
        display_bisect_header(repo)?,
        opt_advice!(user_config.advice_status()?, BISECT_ADVICE),
      ),
    };

    write!(out, "{}", header)?;

    // print advice in new paragraph above diffs
    if let Some(advice) = advice {
      write!(out, "\n\n{}", advice)?;
    }

    // compute changes
    match &kind {
      StatusKind::Pick => {
        // cherry picks are weird bc resolved conflicts are not stored in the main
        // repository index. to show meaningful changes you have to diff with the picked
        // commit
        let pick_head = repo.find_reference("CHERRY_PICK_HEAD")?;
        let pick_tree = pick_head.peel_to_tree()?;

        let diff = repo.diff_tree_to_index(Some(&pick_tree), None, None)?;
        let summary = DiffSummary::new(&diff)?.non_conflicts();

        if !summary.is_empty() {
          write!(
            out,
            "\n\n{} - {}",
            style("Resolved").green(),
            display_summary(&summary, nerdfont)
          )?;
        }
      }

      _ => {
        let show_untracked = match self.no_untracked {
          Some(hide) => !hide,
          None => user_config.status_untracked()?,
        };

        let changes = display_file_statuses(repo, show_untracked, nerdfont)?;
        if !changes.is_empty() {
          write!(
            out,
            "\n\n{}",
            display_file_statuses(repo, show_untracked, nerdfont)?
          )?;
        }
      }
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

    write!(out, "{}", style(name).cyan())?;

    let proj_repo = match Repository::open(&project.path).repo_not_found_ok()? {
      Some(it) => it,
      None => {
        write!(out, " {}", style("not initialized").dim())?;
        return Ok(out);
      }
    };

    let untracked = UserConfig::new(&proj_repo)?.status_untracked()?;
    if has_workdir_changes(&proj_repo, untracked)? {
      write!(out, " {}", style("●").yellow())?;
    }

    if let Some(head) = get_head_resolved(&proj_repo)? {
      let commit = head.peel_to_commit()?;

      if head.is_branch() {
        let branch_info = BranchInfo::from_reference(&head)?;
        write!(out, " on {}", style(branch_info.name()).green())?;

        if let Some(upstream) = branch_info.upstream(&proj_repo)? {
          let upstream_tip = upstream.get().peel_to_commit()?.id();
          let (ahead, behind) = proj_repo.graph_ahead_behind(commit.id(), upstream_tip)?;

          write!(
            out,
            " {}",
            dim_brackets!("{}", display_plus_minus(ahead, behind))
          )?;
        }
      }

      write!(
        out,
        " {} · {}",
        display_time(&commit.time(), &DisplayTimeOptions::try_from(&config)?)?,
        commit.summary()?.unwrap_or(commit.message()?)
      )?;

      if is_term() {
        out = truncate_str(&out, get_term_width(), "\u{2026}").to_string();
      }

      if config.show_authorship()? {
        write!(
          out,
          "{}",
          self.display_different_signature(repo, &proj_repo)?
        )?;
      }
    };

    Ok(out)
  }

  /// Builds output for a particular submodule. `repo` is the parent repo, not
  /// the submodule.
  fn display_module(&self, repo: &Repository, mod_name: &str) -> Result<String> {
    let mut out = String::new();

    write!(out, "{}", style(mod_name).cyan())?;

    let config = UserConfig::new(repo)?;
    let module = repo.find_submodule(mod_name)?;

    let mod_repo = match module.open().repo_not_found_ok()? {
      Some(it) => it,
      None => {
        write!(out, " {}", style("not initialized").dim())?;
        return Ok(out);
      }
    };

    let untracked = UserConfig::new(&mod_repo)?.status_untracked()?;
    if has_workdir_changes(&mod_repo, untracked)? {
      write!(out, " {}", style("●").yellow())?;
    }

    // committed state of submodule (commit parent expects module to be on)
    let head_id = module.head_id();
    // current state of submodule (commit module is actually on)
    let index_id = module.index_id();

    match (index_id, head_id) {
      (Some(index_id), Some(head_id)) => {
        let (ahead, behind) = mod_repo.graph_ahead_behind(index_id, head_id)?;
        write!(
          out,
          " {}",
          dim_brackets!("{}", display_plus_minus(ahead, behind))
        )?;
      }

      (Some(_), None) => {
        write!(out, " {}", style("untracked").red())?;
      }

      _ => (),
    }

    // actual repo info
    if let Some(head) = get_head_resolved(&mod_repo)? {
      let commit = head.peel_to_commit()?;

      if head.is_branch() {
        let name = head.shorthand_bytes().to_str_lossy();
        write!(out, " on {}", style(&name).green())?;
      }

      write!(
        out,
        " {} · {}",
        display_time(&commit.time(), &DisplayTimeOptions::try_from(&config)?)?,
        commit.summary()?.unwrap_or(commit.message()?)
      )?;

      if is_term() {
        out = truncate_str(&out, get_term_width(), "\u{2026}").to_string();
      }

      if config.show_authorship()? {
        write!(
          out,
          "{}",
          self.display_different_signature(repo, &mod_repo)?
        )?;
      }
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
        write!(out, "\n  {}", display_signature(Some(&child_sig)))?;
      }
    } else {
      // default text for when no name/email is found
      write!(out, "\n  {}", display_signature(None))?;
    }

    Ok(out)
  }
}

/// Displays a header when there is no other active operation (e.g. rebase/merge
/// conflicts). Shows current branch, commit it points to, and upstream/base
/// info if available. Unlike the others, this header takes up to 3 lines.
fn display_normal_header(
  repo: &Repository,
  proj_config: &ProjectConfig,
  user_config: &UserConfig,
  head: &CheckoutStatus,
  head_refname: &str,
) -> Result<String> {
  let mut out = String::with_capacity(80);
  let nerdfont = user_config.nerdfont()?;

  let handle = ThreadedRepoHandle::from(repo);

  let rf = repo.find_reference(head_refname)?;

  match head {
    CheckoutStatus::NoCommits => {
      let branch_name = rf
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

    _ => {
      // all other types need some common info
      let commit = rf.peel_to_commit()?;
      let commit_id = commit.id();

      // small bites of supplementary info, displayed in brackets next to the
      // name
      //
      // e.g. local branches:
      // [up: +0 -0 | base: +0 -0 | ver: v1.0.0 +13 | wips: 2]
      //
      // with nerdfont:
      // [ +0 -0 |  +0 -0 |  v1.0.0 +13 | 󱉚 2]
      let mut extras = Vec::new();

      // gets current version info
      let version_extra = |repo: &Repository, id: Oid| -> Result<Option<String>> {
        if let Some((version, since)) = find_current_version(repo, proj_config, id)? {
          let mut version_extra = format!(
            "{}",
            style!("{} {}", if_nerdfont!(nerdfont, "", "ver:"), version.name()).yellow(),
          );

          if since > 0 {
            version_extra.push_str(&style!(" +{}", since).green().to_string());
          }

          return Ok(Some(version_extra));
        }

        Ok(None)
      };

      match head {
        CheckoutStatus::Branch => {
          let branch = BranchInfo::from_reference(&rf)?;
          write!(out, "On branch {}", style(branch.name()).green())?;

          // run graph traversals in parallel. upstream, base, and version all do a single
          // traversal
          thread::scope(|scope| -> Result<_> {
            let upstream = scope.spawn(|| -> Result<Option<_>> {
              let repo = handle.open()?;
              if let Some(upstream) = branch.upstream(&repo)? {
                let upstream_tip = upstream.get().peel_to_commit()?.id();
                Ok(Some(repo.graph_ahead_behind(upstream_tip, commit_id)?))
              } else {
                Ok(None)
              }
            });

            let base = scope.spawn(|| -> Result<Option<_>> {
              let repo = handle.open()?;
              let config = UserConfig::new(&repo)?;

              if let Some(base) = config.branch_base(branch.name())? {
                let base_tip = base.resolve(&repo)?.peel_to_commit()?.id();
                Ok(Some(repo.graph_ahead_behind(base_tip, commit_id)?))
              } else {
                Ok(None)
              }
            });

            let version = scope.spawn(|| -> Result<Option<_>> {
              let repo = handle.open()?;
              version_extra(&repo, commit_id)
            });

            if let Some((a, b)) = upstream.join().unwrap()? {
              extras.push(format!(
                "{} {}",
                style(if_nerdfont!(nerdfont, "", "up:")).blue(),
                display_plus_minus(a, b)
              ));
            }

            if let Some((a, b)) = base.join().unwrap()? {
              extras.push(format!(
                "{} {}",
                style(if_nerdfont!(nerdfont, "", "base:")).magenta(),
                display_plus_minus(a, b)
              ));
            }

            if let Some(version) = version.join().unwrap()? {
              extras.push(version);
            }

            Ok(())
          })?;

          // wip list doesn't require a traversal and isn't particularly slow
          let wips = WipList::from_branch(repo, branch.name().to_string())?;
          if !wips.is_empty() {
            extras.push(
              style!("{} {}", if_nerdfont!(nerdfont, "󱉚", "wips:"), wips.len())
                .cyan()
                .to_string(),
            );
          }
        }

        CheckoutStatus::Remote => {
          let upstream = BranchInfo::from_reference(&rf)?;
          write!(out, "On upstream {}", style(upstream.name()).green())?;

          thread::scope(|scope| -> Result<_> {
            let ab = scope.spawn(|| -> Result<Option<_>> {
              let repo = handle.open()?;

              if let Some(branch) = find_local_of_upstream(&repo, &upstream)? {
                let branch_tip = branch.get().peel_to_commit()?.id();
                Ok(Some(repo.graph_ahead_behind(branch_tip, commit_id)?))
              } else {
                Ok(None)
              }
            });

            let version = scope.spawn(|| -> Result<Option<_>> {
              let repo = handle.open()?;
              version_extra(&repo, commit_id)
            });

            if let Some((a, b)) = ab.join().unwrap()? {
              extras.push(display_plus_minus(a, b));
            }

            if let Some(version) = version.join().unwrap()? {
              extras.push(version);
            }

            Ok(())
          })?;
        }

        CheckoutStatus::Tag => {
          let tag_name = rf.shorthand()?;
          write!(out, "On tag {}", style(tag_name).green())?;

          if is_version_tag(repo, proj_config, tag_name)? {
            // current tag is a version tag
            let version = VersionTag::new(tag_name, commit.id());

            // single graph traversal, no need to multithread
            if let Some((prev, since)) = since_prev_version(repo, proj_config, &version)? {
              write!(
                out,
                " {}{} since {}{}",
                style("(").dim(),
                style!("+{}", since).green(),
                style(prev.name()).yellow(),
                style(")").dim(),
              )?;
            }
          } else if let Some(version) = version_extra(repo, commit_id)? {
            // not a version tag, find current version
            extras.push(version);
          }
        }

        CheckoutStatus::Commit => {
          write!(out, "On commit {}", display_hash(commit.as_object())?)?;

          // single graph traversal
          if let Some(version) = version_extra(repo, commit_id)? {
            extras.push(version);
          }
        }

        // we already matched NoCommits
        _ => unreachable!(),
      }

      if !extras.is_empty() {
        let sep = style(" | ").dim().to_string();
        let s = extras.join(&sep);
        write!(out, " {}", dim_brackets!("{}", s))?;
      }

      let commit_line = if let CheckoutStatus::Commit = head {
        format!(
          "{}{}",
          if_nerdfont!(nerdfont, " "),
          display_commit_compact(&commit, user_config, false)?
        )
      } else {
        display_commit_compact(&commit, user_config, true)?
      };

      write!(
        out,
        "\n{}",
        trunc_term_width(&commit_line, &style("\u{2026}").dim().to_string())
      )?;
    }
  };

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
fn display_rebase_header(repo: &Repository, info: &RebaseInfo) -> Result<String> {
  let branch_ref = repo.resolve_reference_from_short_name(info.head().trim())?;
  let branch_name = branch_ref.shorthand()?;

  let base_commit = repo.find_commit(info.onto())?;

  // try to find a matching branch, but don't error
  let base = match find_branch_at_commit(repo, &base_commit.id()) {
    Ok(branch) => match branch {
      Some(branch) => branch.name()?.map(|it| it.to_string()),
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
    style(branch_name).blue(),
    style(&base).magenta(),
    style!("({}/{})", info.current(), info.total()).dim()
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
/// - `untracked` - whether to show untracked files
/// - `nerdfont` - whether to use nerd font icons or regular characters
pub fn display_file_statuses(repo: &Repository, untracked: bool, nerdfont: bool) -> Result<String> {
  let handle = ThreadedRepoHandle::from(repo);

  // calculate diffs in parallel
  let (conflicts, staged, unstaged) = thread::scope(|scope| -> Result<_> {
    let conflicts = scope.spawn(|| -> Result<Vec<Conflict>> {
      let repo = handle.open()?;
      get_conflicts(&repo)
    });

    let staged = scope.spawn(|| -> Result<DiffSummary> {
      let repo = handle.open()?;
      get_staged_changes(&repo)
    });

    let unstaged = scope.spawn(|| -> Result<DiffSummary> {
      let repo = handle.open()?;
      get_unstaged_changes(&repo, untracked)
    });

    let conflicts = conflicts.join().unwrap()?;
    let staged = staged.join().unwrap()?;
    let unstaged = unstaged.join().unwrap()?;

    Ok((conflicts, staged, unstaged))
  })?;

  // build output
  use std::fmt::Write;
  let mut out = String::new();
  let mut first_paragraph = true;

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
    first_paragraph = false;

    // state that could have conflicts, but there are currently no conflicts
    write!(
      out,
      "{} - {}",
      style("Conflicts").yellow(),
      style("none").green()
    )?;
  }

  if !staged.is_empty() {
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

  if !unstaged.is_empty() {
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
