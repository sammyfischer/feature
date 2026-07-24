use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{CommandFactory, FromArgMatches};
use git2::{ErrorClass, ErrorCode, Repository};

use crate::cli::status::status_guide;
use crate::cli::{Args, Command};
use crate::core::project_config::{ProjectConfig, load_config, load_with_path, local};

mod cli;
mod core;
mod templater;

/// Shared state of the cli
pub struct App {
  /// Fully layered project config struct
  pub config: ProjectConfig,

  /// Path to the project-level config file. May not exist.
  pub config_path: PathBuf,

  pub repo: Repository,

  pub command: Command,
}

impl App {
  pub fn new(args: Args) -> Result<Option<Self>> {
    // completions command ignores git dir entirely
    if let Command::Completions(args) = args.command {
      args.run()?;
      return Ok(None);
    }

    let repo = match Self::find_repo(args.git_dir.as_deref(), args.work_tree.as_deref()) {
      Ok(repo) => Ok(repo) as Result<Repository>,
      Err(e) if e.class() == ErrorClass::Repository && e.code() == ErrorCode::NotFound => {
        // complete command should print an empty list (i.e. nothing) if no repo is
        // found
        if let Command::Complete(_) = args.command {
          return Ok(None);
        }
        // this is an error for any other command
        Err(e.into())
      }
      Err(e) => Err(e.into()),
    }?;

    let (config, config_path) = match args.config {
      // always use command-specified file
      Some(path) => (load_with_path(&path, &repo)?, path),

      // use default file path
      None => (load_config(&repo)?, local::path()),
    };

    Ok(Some(Self {
      config,
      config_path,
      repo,
      command: args.command,
    }))
  }

  fn find_repo(
    git_dir: Option<&Path>,
    work_tree: Option<&Path>,
  ) -> Result<Repository, git2::Error> {
    Ok(match (git_dir, work_tree) {
      // neither, do an automatic search
      (None, None) => Repository::open_from_env()?,

      // just worktree, assume that's the path to the git dir
      (None, Some(wt)) => Repository::open(wt)?,

      // just git dir, open that
      (Some(dir), None) => Repository::open(dir)?,

      // git dir and worktree, open the git dir and set workdir to the worktree
      (Some(dir), Some(wt)) => {
        let repo = Repository::open_bare(dir)?;
        repo.set_workdir(wt, false)?;
        repo
      }
    })
  }
}

fn main() -> Result<()> {
  let mut command = Args::command();
  if let Some(status) = command.find_subcommand_mut("status") {
    *status = status.clone().after_help(status_guide());
  };

  let args = Args::from_arg_matches(&command.get_matches())?;
  let state = App::new(args)?;
  if let Some(state) = state {
    return cli::run(state);
  }
  Ok(())
}
