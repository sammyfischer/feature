use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{CommandFactory, FromArgMatches};
use git2::{ErrorClass, ErrorCode, Repository};

use crate::cli::{Args, Command};
use crate::config::Config;
use crate::util::diff::status_guide;

mod cli;
mod config;
mod data;
mod templater;
mod util;

/// Shared state of the cli
pub struct App {
  /// Fully layered config struct
  pub config: Config,

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
      Some(path) => (config::load_with_path(&path)?, path),

      None => {
        let git_config = repo.config()?.snapshot()?;

        // if this repo is a project
        if data::get_feature_project(&git_config)?.is_some_and(|it| it) {
          let mut cursor = Some(
            repo
              .workdir()
              .unwrap_or_else(|| repo.path())
              .canonicalize()?,
          );

          let file = 'file: {
            // search up for local config file
            while let Some(dir) = cursor {
              // if we found another git dir, assume it's the parent repo
              if dir.join(".git").exists() {
                let file = dir.join("feature.toml");

                // if local config exists in parent
                if file.exists() {
                  break 'file Some(file);
                }
              }

              cursor = dir.parent().map(|path| path.to_owned());
            }

            // found nothing
            None
          };

          match file {
            Some(file) => (config::load_with_path(&file)?, file.to_owned()),
            None => (config::load()?, config::project::path()),
          }
        } else {
          (config::load()?, config::project::path())
        }
      }
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
