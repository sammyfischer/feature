use std::thread;
use std::time::Duration;

use anyhow::Result;
use console::style;
use git2::{Branch, BranchType, Repository};
use indicatif::{MultiProgress, ProgressBar};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::App;
use crate::cli::sync::{
  SyncAction,
  UpdateAction,
  add_sync_spinner,
  display_sync_updates,
  set_sync_spinner_style,
};
use crate::core::branch::{get_current_branch_name, is_merged};
use crate::core::branch_info::BranchInfo;
use crate::core::fetch::fetch_all;
use crate::core::project_config::{self, ProjectConfig};
use crate::core::string::ToStrLossyOwned;
use crate::core::user_config::UserConfig;
use crate::core::wip::WipList;
use crate::core::{NotFoundExt, delete_config_section, open_repo_from_dirs};

const LONG_ABOUT: &str = r"Deletes all branches that:
• have a known base branch
• are an ancestor of their base branch
• have been pushed to a remote
• aren't a protected branch
• aren't the current branch

These checks should prevent most accidental deletions, and at least ensure that
any deleted branches were redundant (being an ancestor of the base means the
base contains the branch's commit history already).";

#[derive(clap::Args, Clone, Debug)]
#[command(
  about = "Deletes merged feature branches",
  long_about = LONG_ABOUT,
  disable_help_subcommand = true
)]
pub struct Args {
  /// Display output but don't delete any branches. Will still fetch all
  /// remotes.
  #[arg(long)]
  dry_run: bool,

  /// Skip automatic fetch of base branch
  #[arg(short = 'F', long, value_name = "SKIP", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  pub no_fetch: Option<bool>,

  /// Don't sync subprojects
  #[arg(long, value_name = "HIDE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  pub no_projects: Option<bool>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    if self.dry_run {
      println!(
        "{}",
        style("Running in dry-run mode, nothing will be deleted").dim()
      );
    }

    let repo_dir = state.repo.path().to_owned();
    let work_dir = state.repo.workdir().to_owned();
    let proj_config = &state.config;
    let user_config = UserConfig::new(&state.repo)?;

    // progress bars
    let multi = MultiProgress::new();
    let mut prefix_width: usize;

    let main_progress = {
      let name = work_dir
        .unwrap_or(&repo_dir)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".to_string());

      prefix_width = name.len();
      let spinner = add_sync_spinner(&multi, name.clone());
      (name, spinner)
    };

    let skip_projects = match self.no_projects {
      Some(it) => it,
      None => !user_config.sync_projects()?,
    };
    let proj_progresses: Vec<_> = if skip_projects {
      Vec::new()
    } else {
      state
        .config
        .projects
        .iter()
        .map(|(name, _)| {
          let name = name.to_owned();
          prefix_width = name.len().max(prefix_width);

          (name.clone(), add_sync_spinner(&multi, name))
        })
        .collect()
    };

    // set spinner templates
    set_sync_spinner_style(&main_progress.1, prefix_width);
    for (_, progress) in &proj_progresses {
      set_sync_spinner_style(progress, prefix_width);
    }

    thread::scope(|scope| -> Result<_> {
      // unlike sync, main repo can run concurrently, since projects won't be
      // reconfigured
      let repo_thread = scope.spawn(|| {
        let repo = open_repo_from_dirs(&repo_dir, work_dir)?;
        let user_config = UserConfig::new(&repo)?;
        self.prune_repo(&repo, &user_config, proj_config, &main_progress.1)
      });

      let proj_thread = scope.spawn(|| {
        if skip_projects {
          Vec::new()
        } else {
          proj_progresses
            .par_iter()
            .map(|(name, progress)| {
              let project = &proj_config.projects[name];
              let repo = Repository::open(&project.path)?;

              let user_config = UserConfig::new(&repo)?;
              let proj_config = project_config::load_with_path(&project.path)?;

              self.prune_repo(&repo, &user_config, &proj_config, progress)
            })
            .collect()
        }
      });

      let main_result = repo_thread.join().unwrap();
      let proj_results = proj_thread.join().unwrap();

      match main_result {
        Ok(SyncAction::Sync(updates)) => {
          let out = display_sync_updates(&main_progress.0, &updates);
          if !out.is_empty() {
            println!("{}", out);
          }
        }
        Err(e) => eprintln!(
          "{} to prune {}: {}",
          style("Failed").red(),
          style(&main_progress.0).cyan(),
          e
        ),
        _ => (),
      }

      for ((name, _), result) in proj_progresses.iter().zip(proj_results) {
        match result {
          Ok(SyncAction::Sync(updates)) => {
            let out = display_sync_updates(name, &updates);
            if !out.is_empty() {
              println!("{}", out);
            }
          }
          Err(e) => eprintln!(
            "{} to prune {}: {}",
            style("Failed").red(),
            style(name).cyan(),
            e
          ),
          _ => (),
        }
      }

      Ok(())
    })
  }

  fn prune_repo(
    &self,
    repo: &Repository,
    user_config: &UserConfig,
    proj_config: &ProjectConfig,
    progress: &ProgressBar,
  ) -> Result<SyncAction> {
    progress.enable_steady_tick(Duration::from_millis(100));

    let skip_fetch = match self.no_fetch {
      Some(it) => it,
      None => !user_config.autofetch()?,
    };

    if !skip_fetch {
      progress.set_message("Fetching all remotes");
      fetch_all(repo)?;
    }

    progress.set_message("Pruning");
    let updates = prune_branches(repo, user_config, proj_config, self.dry_run)?;

    progress.finish_with_message("Pruned");
    Ok(SyncAction::Sync(updates))
  }
}

pub fn prune_branches(
  repo: &Repository,
  user_config: &UserConfig,
  proj_config: &ProjectConfig,
  dry_run: bool,
) -> Result<Vec<UpdateAction>> {
  let branches = repo.branches(Some(BranchType::Local))?;
  let current_name = get_current_branch_name(repo)?;

  let results: Vec<_> = branches
    .flatten()
    .map(|(mut branch, _)| {
      match prune_branch(
        repo,
        user_config,
        proj_config,
        &mut branch,
        current_name.as_deref(),
        dry_run,
      ) {
        Ok(action) => action,
        Err(e) => UpdateAction::Err {
          name: branch
            .name_bytes()
            .map(|name| name.to_str_lossy_owned())
            .unwrap_or("<unknown>".to_string()),
          e: e.to_string(),
        },
      }
    })
    .collect();

  Ok(results)
}

/// Deletes a branch if:
/// - it's not a protected branch
/// - it's not the current branch
/// - it's changes are merged into its base
/// - it was pushed to a remote before
///
/// # Returns
/// Whether the delete operation occured. `false` means the delete didn't occur
/// because the branch was determined to be unsafe to delete, rather than
/// anything going wrong. An error implies that something went wrong.
fn prune_branch(
  repo: &Repository,
  user_config: &UserConfig,
  proj_config: &ProjectConfig,
  branch: &mut Branch,
  current_branch_name: Option<&str>,
  dry_run: bool,
) -> Result<UpdateAction> {
  let info = BranchInfo::from_branch(branch)?;

  // skip protected branches
  if proj_config.protect.iter().any(|it| it == info.name()) {
    return Ok(UpdateAction::None);
  }

  // skip branches that have never been pushed
  match repo.branch_upstream_remote(info.refname()).not_found_ok()? {
    Some(_) => {}
    None => return Ok(UpdateAction::None),
  };

  // find base branch from db, else skip
  let base = match user_config.branch_base(info.name())? {
    Some(base) => base,
    None => return Ok(UpdateAction::None),
  };

  // skip current branch
  if current_branch_name.is_some_and(|it| it == info.name()) {
    // not necessarily an error, but the user should know that a non-protected
    // branch was skipped and may manually need to be deleted
    return Ok(UpdateAction::DeleteSkip {
      name: info.name().to_owned(),
      reason: "currently checked-out".to_owned(),
    });
  }

  // detect if branch is merged (i.e. has no commits that aren't on its base)
  let is_merged = is_merged(repo, branch.get(), &base.resolve(repo)?)?;

  if !is_merged {
    return Ok(UpdateAction::None);
  }

  let commit = branch.get().peel_to_commit()?;

  if !dry_run {
    branch.delete()?;

    // delete wip ref if there was one
    let mut wips = WipList::from_branch(repo, info.name().to_string())?;
    let _ = wips.delete(repo);

    // git2 can't remove entire config sections, but git provides a command to do so
    let key = format!("branch.{}", &info.name());
    let _ = delete_config_section(&key);
  }

  Ok(UpdateAction::Delete {
    name: info.name().to_owned(),
    old: commit.as_object().short_id()?.to_str_lossy_owned(),
  })
}
