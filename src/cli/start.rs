//! Start subcommand

use crate::cli::error::CliError;
use crate::cli::{Cli, CliResult, get_current_branch};
use crate::{await_child, database, git};

#[derive(clap::Args, Clone, Debug)]
pub struct Args {
  #[arg(long, default_value = "-")]
  /// The separator to use when joining words
  pub sep: Option<String>,

  #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
  /// Words to join together as branch name
  pub words: Vec<String>,
}

impl Args {
  pub fn run(&self, cli: &Cli) -> CliResult {
    let sep = self.sep.as_ref().unwrap_or(&cli.config.branch_sep);

    let base = get_current_branch()?;
    if !cli.config.bases.contains(&base) {
      return Err(CliError::Generic(
        "Must call start from a base branch. You can modify base branches with the `feature config` command.".into(),
      ));
    }

    let branch = self.words.join(sep);
    println!("Creating branch: {}", branch);

    await_child!(
      git!("switch", "-c", &branch).spawn()?,
      "git failed to execute"
    )?;

    let db = database::load();

    if let Ok(mut db) = db {
      db.insert(branch, base);

      if database::save(db).is_err() {
        eprintln!("Failed to save branch data to database");
      };
    } else {
      eprintln!("Failed to load database. Couldn't save branch data")
    }

    Ok(())
  }
}
