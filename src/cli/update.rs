use crate::cli::error::CliError;
use crate::cli::{CliResult, get_current_branch};
use crate::{await_child, database, git};

#[derive(clap::Args, Clone, Debug)]
pub struct Args {
  /// Output which base branch will be used, but don't perform the rebase or modify the database.
  #[arg(long)]
  dry_run: bool,

  /// The name of the base branch to use.
  #[arg(
    long_help = "This option will modify the database. If you don't want that, do a regular git rebase.",
    value_name = "BASE_BRANCH"
  )]
  base: Option<String>,
}

impl Args {
  pub fn run(&self) -> CliResult {
    let branch = get_current_branch()?;
    let mut db = database::load()?;

    if let Some(base) = &self.base {
      if self.dry_run {
        println!("Using base: {} (manual)", base);
        return Ok(());
      }

      db.insert(branch, base.to_string());
      return await_child!(git!("rebase", base).spawn()?, "Failed to rebase");
    }

    let base = db.get(&branch).ok_or(CliError::Database(
      "Failed to find base branch from database. Manually specify it in this command or use `feature base`".into(),
    ))?;

    if self.dry_run {
      println!("Using base: {} (database)", base);
      return Ok(());
    }

    await_child!(git!("rebase", base).spawn()?, "Failed to rebase")
  }
}
