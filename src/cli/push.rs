use std::process::Command;

use git2::Repository;

use crate::await_child;
use crate::cli::{Cli, CliResult, get_current_branch, get_tracking_branch};

#[derive(clap::Args, Clone, Debug)]
pub struct Args {
  /// Force push
  #[arg(short, long)]
  force: bool,
}

impl Args {
  pub fn run(&self, cli: &Cli) -> CliResult {
    let repo = Repository::open(".")?;
    let branch = get_current_branch(&repo)?;
    let has_tracking = matches!(get_tracking_branch(&repo, &branch), Ok(it) if !it.is_empty());

    if cli.config.bases.contains(&branch) {
      eprintln!("This is a base branch, refusing to push");
      return Ok(());
    }

    if cli.config.protect.contains(&branch) {
      eprintln!("This is a protected branch, refusing to push");
      return Ok(());
    }

    let mut cmd = Command::new("git");
    cmd.arg("push");

    if self.force {
      cmd.arg("-f");
    } else {
      // protects against overwriting others' work, but allows pushing after rebasing with main
      // (since that changes commit history)
      //
      // NOTE: git2 doesn't have a way of specifying this option, I'd have to reimplement it
      // manually. I'd rather rely on the cli to be more rigorous
      cmd.arg("--force-with-lease");
    }

    if !has_tracking {
      // set upstream. this should be last since we're passing in positional args
      cmd.args(["-u", &cli.config.default_remote, &branch]);
    }

    await_child!(cmd.spawn()?, "Failed to push")
  }
}
