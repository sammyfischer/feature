use anyhow::{Context, Result, anyhow};
use clap::{CommandFactory, FromArgMatches};
use git2::Repository;

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
  pub config: Config,
  pub repo: Repository,
  pub command: Command,
}

impl App {
  pub fn new(args: Args) -> Result<Self> {
    let config = match args.config {
      Some(path) => config::load_with_path(&path),
      None => config::load(),
    }?;

    let repo = match (&args.git_dir, &args.worktree) {
      // neither, do an automatic search
      (None, None) => Repository::open_from_env()?,

      // just worktree, assume that's the path to the git dir
      (None, Some(wt)) => Repository::open(wt)?,

      // just git dir, open that
      (Some(dir), None) => Repository::open(dir)?,

      // git dir and worktree, open the git dir and set workdir to the worktree
      (Some(dir), Some(wt)) => {
        let repo = Repository::open_bare(dir)
          .context("Cannot specify a worktree on a non-bare repository")?;

        if !repo.is_bare() {
          return Err(anyhow!(
            "Cannot specify a worktree on a non-bare repository"
          ));
        }

        repo.set_workdir(wt, false)?;
        repo
      }
    };

    Ok(Self {
      config,
      repo,
      command: args.command,
    })
  }
}

fn main() -> Result<()> {
  let mut command = Args::command();
  if let Some(status) = command.find_subcommand_mut("status") {
    *status = status.clone().after_long_help(status_guide());
  };

  let args = Args::from_arg_matches(&command.get_matches())?;
  let state = App::new(args)?;
  cli::run(state)
}
