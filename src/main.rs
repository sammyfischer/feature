#![feature(trim_prefix_suffix)]

use anyhow::Result;
use clap::Parser;
use git2::Repository;

use crate::cli::{Args, Command};
use crate::config::Config;

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

    let repo = match (args.git_dir, args.worktree) {
      // neither, do an automatic search
      (None, None) => Repository::open_from_env()?,

      // just worktree, assume that's the path to the git dir
      (None, Some(wt)) => Repository::open(wt)?,

      // just git dir, open that
      (Some(dir), None) => Repository::open(dir)?,

      // git dir and worktree, open the git dir and set workdir to the worktree
      (Some(dir), Some(wt)) => {
        let repo = Repository::open(dir)?;
        repo.set_workdir(&wt, false)?;
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
  let args = Args::parse();
  let state = App::new(args)?;
  cli::run(state)
}
