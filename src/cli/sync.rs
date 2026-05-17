use std::fmt::Display;
use std::io::ErrorKind;
use std::path::Path;
use std::{fs, thread};

use anyhow::Result;
use console::style;
use git2::build::RepoBuilder;
use git2::{Branch, BranchType, ErrorClass, ErrorCode, FetchOptions, RemoteCallbacks, Repository};

use crate::cli::prune::prune_branches;
use crate::config::projects::ProjectEntry;
use crate::config::{self, Config};
use crate::util::branch::{fetch_all, get_current_branch_name, hard_reset};
use crate::util::branch_meta::BranchMeta;
use crate::util::diff::{DiffSummary, has_workdir_changes};
use crate::util::string::ToStrLossyOwned;
use crate::util::{credentials_cb, open_repo_from_dirs};
use crate::{App, data, style};

const LONG_ABOUT: &str = r"Updates all branches with their remotes (if they have one), then prunes merged
feature branches.

Branches are fast-forwarded, meaning they may fail to update if their history is
diverged from upstream. That must be resolved manually.

The currently checked-out branch cannot be updated if there are changes in the
working directory. If so, only the current branch will be skipped.

Projects get synced the same way the main repo does.

Submodules get synced by checking-out to their tracked commit.";

#[derive(clap::Args, Clone, Debug)]
#[command(
  about = "Updates branches with their remotes and prunes redundant branches",
  long_about = LONG_ABOUT,
  disable_help_flag = true,
  disable_help_subcommand = true
)]
pub struct Args {
  /// Display output but don't modify any branches. Will still fetch all remotes.
  #[arg(long)]
  pub dry_run: bool,

  /// Skip automatic fetch of base branch
  #[arg(short = 'F', long, value_name = "SKIP", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  pub no_fetch: Option<bool>,

  /// Don't prune after updating
  #[arg(long, value_name = "SKIP", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  pub no_prune: Option<bool>,

  /// Don't sync projects
  #[arg(long, value_name = "HIDE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  pub no_projects: Option<bool>,

  /// Don't sync submodules
  #[arg(short = 'M', long, value_name = "HIDE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  pub no_modules: Option<bool>,
}

/// What happened in an attempt to sync a repo
pub enum SyncAction {
  /// For projects/modules, the repo was cloned
  Clone,

  /// The repo's refs were synced
  Sync(Vec<UpdateAction>),

  /// For modules, the repo was check-out somewhere else
  Checkout { old: String, new: String },
}

/// What happened in an attempt to sync a particular branch in a repo
pub enum UpdateAction {
  /// The reference was fast-forwarded
  Update {
    name: String,
    old: String,
    changes: DiffSummary,
  },
  UpdateSkip {
    name: String,
    reason: String,
  },

  /// The reference was deleted
  Delete {
    name: String,
    old: String,
  },
  DeleteSkip {
    name: String,
    reason: String,
  },

  /// The action was skipped for a reason that's irrelevant to the user
  None,

  /// An error occured
  Err {
    name: String,
    e: String,
  },
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    if self.dry_run {
      println!(
        "{}",
        style("Running in dry-run mode, no branches will be updated or deleted").dim()
      );
    }

    let repo_dir = state.repo.path().to_owned();
    let work_dir = state.repo.workdir().to_owned();
    let app_config = &state.config;
    let git_config = state.repo.config()?.snapshot()?;
    let root = state
      .repo
      .workdir()
      .unwrap_or_else(|| state.repo.path())
      .to_owned();

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

    let skip_modules = match self.no_modules {
      Some(it) => it,
      None => !data::get_sync_modules(&git_config)?,
    };
    let mod_names: Vec<_> = if skip_modules {
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
      // SYNC PARENT REPO
      let repo_thread = scope.spawn(|| {
        let repo = open_repo_from_dirs(&repo_dir, work_dir)?;
        self.sync_repo(&repo, app_config)
      });

      // SYNC ALL PROJECTS
      let proj_thread = scope.spawn(|| {
        use rayon::prelude::*;
        if skip_projects {
          Vec::new()
        } else {
          app_config
            .projects
            .par_iter()
            .map(|(_, project)| self.sync_or_clone_project(&root, project))
            .collect()
        }
      });

      // SYNC ALL MODULES
      let mod_thread = scope.spawn(|| {
        use rayon::prelude::*;
        if skip_modules {
          Vec::new()
        } else {
          mod_names
            .par_iter()
            .map(|mod_name| -> Result<Option<SyncAction>> {
              let repo = match &work_dir {
                Some(work_dir) => {
                  let repo = Repository::open_bare(&repo_dir)?;
                  repo.set_workdir(work_dir, false)?;
                  repo
                }
                None => Repository::open(&repo_dir)?,
              };
              self.sync_module(&repo, mod_name)
            })
            .collect()
        }
      });

      // collect and display
      let main_result = repo_thread.join().unwrap();
      match main_result {
        Ok(action) => println!("{}", display_sync_action("repo", &action)),
        Err(e) => eprintln!(
          "{} to sync {}: {}",
          style("Failed"),
          style("repo").cyan(),
          e
        ),
      }

      let proj_results = proj_thread.join().unwrap();
      for result in proj_names.iter().zip(proj_results) {
        let (name, result) = result;
        match result {
          Ok(action) => println!("{}", display_sync_action(name, &action)),
          Err(e) => eprintln!("{} to sync {}: {}", style("Failed"), style(name).cyan(), e),
        }
      }

      let mod_results = mod_thread.join().unwrap();
      for result in mod_names.iter().zip(mod_results) {
        let (name, result) = result;
        match result {
          Ok(Some(action)) => println!("{}", display_sync_action(name, &action)),
          Ok(None) => (),
          Err(e) => eprintln!("{} to sync {}: {}", style("Failed"), style(name).cyan(), e),
        }
      }

      Ok(())
    })
  }

  fn sync_repo(&self, repo: &Repository, config: &Config) -> Result<SyncAction> {
    let git_config = repo.config()?.snapshot()?;
    let skip_fetch = match self.no_fetch {
      Some(it) => it,
      None => !data::get_feature_autofetch(&git_config)?,
    };

    if !skip_fetch {
      fetch_all(repo)?;
    }

    let current_branch = get_current_branch_name(repo)?;
    let branches = repo.branches(Some(BranchType::Local))?;

    let mut updates: Vec<UpdateAction> = branches
      .flatten()
      .map(|(mut branch, _)| -> UpdateAction {
        match self.update_branch(repo, &mut branch, current_branch.as_deref(), self.dry_run) {
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

    let skip_prune = match self.no_prune {
      Some(it) => it,
      None => !data::get_sync_prune(&git_config)?,
    };

    if !skip_prune {
      let results = prune_branches(repo, config, self.dry_run)?;
      for action in results {
        updates.push(action);
      }
    }

    Ok(SyncAction::Sync(updates))
  }

  fn sync_or_clone_project(&self, root: &Path, project: &ProjectEntry) -> Result<SyncAction> {
    match Repository::open(&project.path) {
      // already cloned, sync it
      Ok(repo) => {
        let config = config::load_with_path(&project.path)?;
        self.sync_repo(&repo, &config)
      }

      // doesn't exist, just clone and continue
      Err(e)
        if (e.class() == ErrorClass::Os && e.code() == ErrorCode::NotFound)
          || (e.class() == ErrorClass::Repository && e.code() == ErrorCode::NotFound) =>
      {
        // make sure path exists
        let path = root.join(&project.path);
        match fs::create_dir_all(&path) {
          Ok(_) => (),
          Err(e) if e.kind() == ErrorKind::AlreadyExists => (),
          Err(e) => return Err(e.into()),
        }

        let mut builder = RepoBuilder::new();
        let mut fetch_opts = FetchOptions::new();
        let mut remote_cbs = RemoteCallbacks::new();
        remote_cbs.credentials(credentials_cb);
        fetch_opts.remote_callbacks(remote_cbs);
        builder.fetch_options(fetch_opts);

        builder.clone(&project.url, &path)?;
        Ok(SyncAction::Clone)
      }

      Err(e) => Err(e.into()),
    }
  }

  /// Syncs a module by bringing it to the currently tracked commit
  fn sync_module(&self, repo: &Repository, mod_name: &str) -> Result<Option<SyncAction>> {
    let module = repo.find_submodule(mod_name)?;
    let mod_repo = module.open()?;

    let head_id = module.head_id();
    let index_id = module.index_id();

    // checkout the expected commit
    if let (Some(expected), Some(current)) = (head_id, index_id) {
      let old_commit = mod_repo.find_commit(current)?;
      let new_commit = mod_repo.find_commit(expected)?;
      let tree = new_commit.tree()?;
      mod_repo.checkout_tree(tree.as_object(), None)?;

      Ok(Some(SyncAction::Checkout {
        old: old_commit.as_object().short_id()?.to_str_lossy_owned(),
        new: new_commit.as_object().short_id()?.to_str_lossy_owned(),
      }))
    } else {
      Ok(None)
    }
  }

  /// Fast-forwards a branch to match upstream. Set `current` to true when
  /// fast-forwarding the currently checked-out branch, so that HEAD and the
  /// workdir are correctly updated.
  ///
  /// # Returns
  /// If the branch was skipped for a reason determined to be irrelevant to the
  /// user, returns `None`. Otherwise, returns the [UpdateAction].
  fn update_branch(
    &self,
    repo: &Repository,
    branch: &mut Branch,
    current_branch: Option<&str>,
    dry_run: bool,
  ) -> Result<UpdateAction> {
    let branch_meta = BranchMeta::from_branch(branch)?;
    let is_current = current_branch
      .as_ref()
      .is_some_and(|it| *it == branch_meta.name());

    let upstream = branch_meta.upstream(repo)?;
    let Some(upstream) = upstream else {
      // no upstream, nothing to update
      return Ok(UpdateAction::None);
    };
    let upstream_meta = BranchMeta::from_branch(&upstream)?;

    if is_current {
      // check for local changes
      if has_workdir_changes(repo)? {
        return Ok(UpdateAction::UpdateSkip {
          name: branch_meta.name().to_owned(),
          reason: "local changes".to_owned(),
        });
      }
    }

    let branch_tip = branch.get().peel_to_commit()?;
    let upstream_tip = upstream.get().peel_to_commit()?;

    // already up to date
    if branch_tip.id() == upstream_tip.id() {
      return Ok(UpdateAction::None);
    }

    let can_ff = repo.graph_descendant_of(upstream_tip.id(), branch_tip.id())?;

    if !can_ff {
      return Ok(UpdateAction::UpdateSkip {
        name: branch_meta.name().to_owned(),
        reason: "not fast-forwardable".to_string(),
      });
    }

    let mut diff =
      repo.diff_tree_to_tree(Some(&branch_tip.tree()?), Some(&upstream_tip.tree()?), None)?;
    diff.find_similar(None)?;
    let changes = DiffSummary::new(&diff)?;

    if !dry_run {
      if is_current {
        // to update the current branch, we also need to update HEAD. this is just a hard reset
        hard_reset(repo, upstream.get())?;
      } else {
        // for other branches, we just move them to the upstream commit
        branch.get_mut().set_target(
          upstream_tip.id(),
          &format!("feature sync: fast-forward to {}", upstream_meta.refname()),
        )?;
      }
    }

    Ok(UpdateAction::Update {
      name: branch_meta.name().to_owned(),
      old: branch_tip.as_object().short_id()?.to_str_lossy_owned(),
      changes,
    })
  }
}

pub fn display_sync_action(name: &str, action: &SyncAction) -> String {
  match action {
    SyncAction::Clone => format!(
      "{} {}",
      style("Cloned").bold().green(),
      style(name).bold().cyan()
    ),

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
        style("Synced").bold().green(),
        style(name).bold().cyan(),
        msg
      )
    }

    SyncAction::Checkout { old, new } => format!(
      "{} {} to {} {}",
      style("Checked-out").bold().green(),
      style(name).bold().cyan(),
      new,
      style!("(was {})", old)
    ),
  }
}

impl Display for UpdateAction {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      UpdateAction::Update { name, old, changes } => {
        write!(
          f,
          "{} {} {} | {}",
          style("Updated").green(),
          style(name).cyan(),
          style!("(was {})", old).dim(),
          changes.display_header()
        )
      }

      UpdateAction::UpdateSkip { name, reason } => {
        write!(
          f,
          "{} updating {}: {}",
          style("Skipped").yellow(),
          style(name).cyan(),
          reason
        )
      }

      UpdateAction::Delete { name, old } => {
        write!(
          f,
          "{} {} {}",
          style("Deleted").red(),
          style(name).cyan(),
          style!("(was {})", old).dim()
        )
      }

      UpdateAction::DeleteSkip { name, reason } => {
        write!(
          f,
          "{} pruning {}: {}",
          style("Skipped").yellow(),
          style(name).cyan(),
          reason
        )
      }

      UpdateAction::None => Ok(()),

      UpdateAction::Err { name, e } => {
        write!(
          f,
          "{} to update {}: {}",
          style("Failed").red(),
          style(name).cyan(),
          e
        )
      }
    }
  }
}
