use std::thread;

use anyhow::Result;
use console::style;
use git2::{Branch, BranchType, ErrorCode, Repository};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::cli::sync::{SyncAction, UpdateAction};
use crate::config::{self, Config};
use crate::util::branch::{fetch_all, get_current_branch_name, is_merged};
use crate::util::branch_meta::BranchMeta;
use crate::util::string::ToStrLossyOwned;
use crate::util::{delete_config_section, open_repo_from_dirs};
use crate::{App, data};

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
  disable_help_flag = true,
  disable_help_subcommand = true
)]
pub struct Args {
  /// Display output but don't delete any branches. Will still fetch all remotes.
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
    let app_config = &state.config;
    let git_config = state.repo.config()?.snapshot()?;

    let skip_projects = match self.no_projects {
      Some(it) => it,
      None => !data::get_sync_projects(&git_config)?,
    };
    let proj_names: Vec<_> = if skip_projects {
      Vec::new()
    } else {
      state
        .config
        .projects
        .iter()
        .map(|(name, _)| name.to_owned())
        .collect()
    };

    thread::scope(|scope| -> Result<_> {
      let repo_thread = scope.spawn(|| {
        let repo = open_repo_from_dirs(&repo_dir, work_dir)?;
        self.prune_repo(&repo, app_config)
      });

      let proj_thread = scope.spawn(|| {
        if skip_projects {
          Vec::new()
        } else {
          app_config
            .projects
            .par_iter()
            .map(|(_, project)| {
              let repo = Repository::open(&project.path)?;
              let config = config::load_with_path(&project.path)?;
              self.prune_repo(&repo, &config)
            })
            .collect()
        }
      });

      let repo_result = repo_thread.join().unwrap();
      match repo_result {
        Ok(action) => println!("{}", display_prune_action("repo", &action)),
        Err(e) => eprintln!(
          "{} to prune {}: {}",
          style("Failed").red(),
          style("repo").cyan(),
          e
        ),
      }

      let proj_results = proj_thread.join().unwrap();
      for (name, result) in proj_names.iter().zip(proj_results) {
        match result {
          Ok(action) => println!("{}", display_prune_action(name, &action)),
          Err(e) => eprintln!(
            "{} to prune {}: {}",
            style("Failed").red(),
            style(name).cyan(),
            e
          ),
        }
      }

      Ok(())
    })
  }

  fn prune_repo(&self, repo: &Repository, config: &Config) -> Result<SyncAction> {
    let skip_fetch = match self.no_fetch {
      Some(it) => it,
      None => !data::get_feature_autofetch(&repo.config()?.snapshot()?)?,
    };

    if !skip_fetch {
      fetch_all(repo)?;
    }

    Ok(SyncAction::Sync(prune_branches(
      repo,
      config,
      self.dry_run,
    )?))
  }
}

pub fn prune_branches(
  repo: &Repository,
  config: &Config,
  dry_run: bool,
) -> Result<Vec<UpdateAction>> {
  let branches = repo.branches(Some(BranchType::Local))?;
  let current_name = get_current_branch_name(repo)?;

  let results: Vec<_> = branches
    .flatten()
    .map(|(mut branch, _)| {
      match prune_branch(repo, config, &mut branch, current_name.as_deref(), dry_run) {
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
/// Whether the delete operation occured. `false` means the delete didn't occur because the branch
/// was determined to be unsafe to delete, rather than anything going wrong. An error implies that
/// something went wrong.
fn prune_branch(
  repo: &Repository,
  config: &Config,
  branch: &mut Branch,
  current_branch_name: Option<&str>,
  dry_run: bool,
) -> Result<UpdateAction> {
  let meta = BranchMeta::from_branch(branch)?;

  // skip protected branches
  if config.protect.iter().any(|it| it == meta.name()) {
    return Ok(UpdateAction::None);
  }

  // skip branches that have never been pushed
  match repo.branch_upstream_remote(meta.refname()) {
    Ok(_) => {}
    Err(e) if e.code() == ErrorCode::NotFound => return Ok(UpdateAction::None),
    Err(e) => return Err(e.into()),
  }

  // find base branch from db, else skip
  let base = match data::get_feature_base(repo, meta.name())? {
    Some(base) => base,
    None => return Ok(UpdateAction::None),
  };

  // skip current branch
  if current_branch_name.is_some_and(|it| it == meta.name()) {
    // not necessarily an error, but the user should know that a non-protected branch was
    // skipped and may manually need to be deleted
    return Ok(UpdateAction::DeleteSkip {
      name: meta.name().to_owned(),
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
  }

  // git2 can't remove entire config sections, but git provides a command to do so
  let key = format!("branch.{}", &meta.name());
  let _ = delete_config_section(&key);

  Ok(UpdateAction::Delete {
    name: meta.name().to_owned(),
    old: commit.as_object().short_id()?.to_str_lossy_owned(),
  })
}

/// Displays this [SyncAction::Sync], but uses the word "Pruned" instead of
/// "Synced".
///
/// # Panics
/// This must only be called on a [SyncAction::Sync]. Panics when called on
/// another other [SyncAction] type.
pub fn display_prune_action(name: &str, action: &SyncAction) -> String {
  match action {
    SyncAction::Sync(updates) => {
      let updates: Vec<_> = updates
        .iter()
        .filter_map(|update| {
          if let UpdateAction::None = update {
            None
          } else {
            Some(format!("  {}", update))
          }
        })
        .collect();

      let msg = if updates.is_empty() {
        " up to date".to_string()
      } else {
        format!("\n{}", updates.join("\n"))
      };

      format!(
        "{} {}:{}",
        style("Pruned").bold().green(),
        style(name).bold().cyan(),
        msg
      )
    }

    _ => panic!("Illegal SyncAction type"),
  }
}
