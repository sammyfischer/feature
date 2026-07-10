use std::fmt::Display;
use std::io::ErrorKind;
use std::time::Duration;
use std::{fs, thread};

use anyhow::Result;
use console::style;
use git2::build::RepoBuilder;
use git2::{
  Branch,
  BranchType,
  ErrorClass,
  ErrorCode,
  FetchOptions,
  RemoteCallbacks,
  Repository,
  SubmoduleUpdateOptions,
};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::cli::prune::prune_branches;
use crate::config::projects::ProjectEntry;
use crate::config::{self, Config};
use crate::util::branch::{fetch_all, get_current_branch_name, hard_reset};
use crate::util::branch_meta::BranchMeta;
use crate::util::diff::{DiffSummary, has_workdir_changes};
use crate::util::get_credentials_cb;
use crate::util::string::ToStrLossyOwned;
use crate::util::term::TICK_STRINGS;
use crate::util::wip::{WIP_NAMESPACE, get_wip_refname};
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
  disable_help_subcommand = true
)]
pub struct Args {
  /// Display output but don't modify any branches. Will still fetch all
  /// remotes.
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
  /// The repo's refs were synced
  Sync(Vec<UpdateAction>),

  /// For projects, the repo was cloned
  ProjectInit,

  ModuleUpdate,
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

    // project names and bars
    let skip_projects = match self.no_projects {
      Some(it) => it,
      None => !data::get_sync_projects(&git_config)?,
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

    let skip_modules = match self.no_modules {
      Some(it) => it,
      None => !data::get_sync_modules(&git_config)?,
    };
    let mod_progresses: Vec<_> = if skip_modules {
      Vec::new()
    } else {
      state
        .repo
        .submodules()?
        .iter()
        .map(|module| {
          let name = module.name_bytes().to_str_lossy_owned();
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
    for (_, progress) in &mod_progresses {
      set_sync_spinner_style(progress, prefix_width);
    }

    // main repo must run first, since projects/submodule state could change as a
    // result
    let main_result = self.sync_repo(&state.repo, app_config, &main_progress.1)?;

    let (proj_results, mod_results) = thread::scope(|scope| -> Result<_> {
      // SYNC ALL PROJECTS
      let proj_thread = scope.spawn(|| {
        if skip_projects {
          Vec::new()
        } else {
          proj_progresses
            .par_iter()
            .map(|(name, progress)| {
              let project = &app_config.projects[name];
              self.sync_or_clone_project(project, progress)
            })
            .collect()
        }
      });

      // SYNC ALL MODULES
      let mod_thread = scope.spawn(|| {
        if skip_modules {
          Vec::new()
        } else {
          mod_progresses
            .par_iter()
            .map(|(name, progress)| -> Result<SyncAction> {
              let repo = match &work_dir {
                Some(work_dir) => {
                  let repo = Repository::open_bare(&repo_dir)?;
                  repo.set_workdir(work_dir, false)?;
                  repo
                }
                None => Repository::open(&repo_dir)?,
              };
              self.update_module(&repo, name, progress)
            })
            .collect()
        }
      });

      // collect and display
      let proj_results = proj_thread.join().unwrap();
      let mod_results = mod_thread.join().unwrap();

      Ok((proj_results, mod_results))
    })?;

    // main repo summary
    if let SyncAction::Sync(updates) = &main_result {
      let out = display_sync_updates(&main_progress.0, updates);
      if !out.is_empty() {
        println!("{}", out)
      };
    }

    // project summaries/errors
    for result in proj_progresses.iter().zip(proj_results) {
      let ((name, _), result) = result;
      match result {
        Ok(SyncAction::Sync(updates)) => {
          let out = display_sync_updates(name, &updates);
          if !out.is_empty() {
            println!("{}", out);
          }
        }
        Err(e) => eprintln!("{} to sync {}: {}", style("Failed"), style(name).cyan(), e),
        _ => (),
      }
    }

    // submodule errors
    for result in mod_progresses.iter().zip(mod_results) {
      let ((name, _), result) = result;
      if let Err(e) = result {
        eprintln!("{} to sync {}: {}", style("Failed"), style(name).cyan(), e)
      }
    }

    Ok(())
  }

  fn sync_repo(
    &self,
    repo: &Repository,
    config: &Config,
    progress: &ProgressBar,
  ) -> Result<SyncAction> {
    progress.enable_steady_tick(Duration::from_millis(100));

    let git_config = repo.config()?.snapshot()?;
    let skip_fetch = match self.no_fetch {
      Some(it) => it,
      None => !data::get_feature_autofetch(&git_config)?,
    };

    if !skip_fetch {
      progress.set_message("Fetching all remotes");
      fetch_all(repo)?;
    }

    progress.set_message("Updating branches");
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
      progress.set_message("Pruning branches");
      let results = prune_branches(repo, config, self.dry_run)?;
      for action in results {
        updates.push(action);
      }
    }

    progress.set_message("Cleaning up");

    // iterate through all wip refs. delete them if their backing branch was
    // deleted
    let refs = repo.references_glob(&get_wip_refname("*"))?;
    for r in refs {
      let mut r = r?;
      let branch_name = r
        .name()?
        .strip_prefix(&format!("{}/", WIP_NAMESPACE))
        .expect("Invalid wip refname");

      match repo.find_reference(&format!("refs/heads/{}", branch_name)) {
        // branch was deleted, cleanup wip
        Err(e) if e.code() == ErrorCode::NotFound => r.delete()?,

        // branch exists or different error, do nothing
        _ => {}
      }
    }

    progress.finish_with_message("Synced");
    Ok(SyncAction::Sync(updates))
  }

  fn sync_or_clone_project(
    &self,
    project: &ProjectEntry,
    progress: &ProgressBar,
  ) -> Result<SyncAction> {
    match Repository::open(&project.path) {
      // already cloned, sync it
      Ok(repo) => {
        let app_config = config::load_with_path(&project.path)?;
        let mut git_config = repo.config()?;
        git_config.set_bool("feature.project", true)?;
        self.sync_repo(&repo, &app_config, progress)
      }

      // doesn't exist, just clone and continue
      Err(e)
        if (e.class() == ErrorClass::Os && e.code() == ErrorCode::NotFound)
          || (e.class() == ErrorClass::Repository && e.code() == ErrorCode::NotFound) =>
      {
        progress.set_message("Cloning project");
        progress.enable_steady_tick(Duration::from_millis(100));

        // make sure path exists
        match fs::create_dir_all(&project.path) {
          Ok(_) => (),
          Err(e) if e.kind() == ErrorKind::AlreadyExists => (),
          Err(e) => return Err(e.into()),
        }

        let mut builder = RepoBuilder::new();
        let mut fetch_opts = FetchOptions::new();
        let mut remote_cbs = RemoteCallbacks::new();
        remote_cbs.credentials(get_credentials_cb());
        fetch_opts.remote_callbacks(remote_cbs);
        builder.fetch_options(fetch_opts);

        let repo = builder.clone(&project.url, &project.path)?;
        let mut config = repo.config()?;
        config.set_bool("feature.project", true)?;

        progress.finish_with_message("Cloned project");
        Ok(SyncAction::ProjectInit)
      }

      Err(e) => Err(e.into()),
    }
  }

  /// Syncs a module similar to `git submodule update --init` and `git submodule
  /// sync`.
  fn update_module(
    &self,
    repo: &Repository,
    mod_name: &str,
    progress: &ProgressBar,
  ) -> Result<SyncAction> {
    let mut module = repo.find_submodule(mod_name)?;
    let mod_repo = match module.open() {
      Ok(repo) => Some(repo),
      Err(e)
        if (e.class() == ErrorClass::Os || e.class() == ErrorClass::Repository)
          && e.code() == ErrorCode::NotFound =>
      {
        None
      }
      Err(e) => return Err(e.into()),
    };

    progress.set_message("Updating module");
    progress.enable_steady_tick(Duration::from_millis(100));

    // if repo, head_id, and index_id all exist, then get expected and actual commit
    // (short ids)
    let (expected, actual) = if let (Some(mod_repo), Some(expect_id), Some(actual_id)) =
      (mod_repo.as_ref(), module.head_id(), module.index_id())
    {
      (
        Some(
          mod_repo
            .find_commit(expect_id)?
            .as_object()
            .short_id()?
            .to_str_lossy_owned(),
        ),
        Some(
          mod_repo
            .find_commit(actual_id)?
            .as_object()
            .short_id()?
            .to_str_lossy_owned(),
        ),
      )
    } else {
      (None, None)
    };

    let mut opts = SubmoduleUpdateOptions::new();
    let mut fetch_opts = FetchOptions::new();
    let mut remote_cbs = RemoteCallbacks::new();
    remote_cbs.credentials(get_credentials_cb());
    fetch_opts.remote_callbacks(remote_cbs);
    opts.allow_fetch(true);
    opts.fetch(fetch_opts);
    module.update(true, Some(&mut opts))?;

    // sync submodule's remote url
    module.sync()?;

    if let (Some(expected), Some(actual)) = (expected, actual) {
      progress.finish_with_message(format!(
        "Updated module {} -> {}",
        style(&actual).yellow(),
        style(&expected).yellow()
      ));
    } else {
      // if we failed to get expected/actual before, assume it was just initialized
      progress.finish_with_message("Initialized module");
    }

    Ok(SyncAction::ModuleUpdate)
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
        // to update the current branch, we also need to update HEAD. this is just a
        // hard reset
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

/// Adds a new unstyled spinner to the [MultiProgress]. Returns the newly
/// created spinner.
pub fn add_sync_spinner(multi: &MultiProgress, name: String) -> ProgressBar {
  let spinner = multi.add(ProgressBar::new_spinner());
  spinner.set_prefix(name);
  spinner.set_message("Starting");
  spinner
}

/// Sets the style of the spinner
pub fn set_sync_spinner_style(progress: &ProgressBar, prefix_width: usize) {
  progress.set_style(
    ProgressStyle::with_template(&format!(
      "{{prefix:<{prefix_width}.cyan}} {{spinner:.green}} {{elapsed:.dim}} {{msg}}"
    ))
    .expect("Invalid format for progress bar template")
    .tick_strings(&TICK_STRINGS),
  );
}

/// Displays a list of [UpdateAction]s along with a header line containing
/// `name`. [UpdateAction::None] is filtered out first. If there are no
/// remaning updates, an empty string is returned.
pub fn display_sync_updates(name: &str, updates: &[UpdateAction]) -> String {
  use std::fmt::Write;
  let mut out = String::new();

  let updates: Vec<&UpdateAction> = updates
    .iter()
    .filter(|update| match update {
      UpdateAction::Update { .. }
      | UpdateAction::UpdateSkip { .. }
      | UpdateAction::Delete { .. }
      | UpdateAction::DeleteSkip { .. }
      | UpdateAction::Err { .. } => true,

      // filter out None action
      UpdateAction::None => false,
    })
    .collect();

  if !updates.is_empty() {
    let _ = write!(out, "{} changes:", style(name).cyan());
  }

  for update in updates {
    let _ = write!(out, "\n  {}", update);
  }

  out
}

impl Display for UpdateAction {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      UpdateAction::Update { name, old, changes } => {
        write!(
          f,
          "{} {} {} | {}",
          style("Updated").green(),
          name,
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
          name,
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
