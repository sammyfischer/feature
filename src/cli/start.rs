//! Start subcommand

use git2::Repository;

use crate::cli::{Cli, CliResult, get_current_branch, get_current_commit};
use crate::{cli_err, database};

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
    let repo = Repository::open_from_env()?;
    let sep = self.sep.as_ref().unwrap_or(&cli.config.branch_sep);

    let base = get_current_branch(&repo)?;
    if !cli.config.bases.contains(&base) {
      return Err(cli_err!(
        Generic,
        "Must call start from a base branch. You can modify base branches with the `feature config` command."
      ));
    }

    let branch_name = self.words.join(sep);
    println!("Creating branch: {}", branch_name);

    // find commit to create branch on
    let current_commit =
      get_current_commit(&repo).map_err(|e| cli_err!(Git, "Failed to find current commit: {e}"))?;

    // create branch
    let branch = repo
      .branch(&branch_name, &current_commit, false)
      .map_err(|e| cli_err!(Git, "Failed to create branch: {e}"))?;

    // get tree to checkout
    let tree = branch
      .get()
      .peel_to_tree()
      .map_err(|e| cli_err!(Git, "Failed to resolve branch as tree: {e}"))?;

    // checkout branch
    repo
      .checkout_tree(tree.as_object(), None)
      .map_err(|e| cli_err!(Git, "Failed to switch to branch: {e}"))?;

    // update HEAD
    repo
      .set_head(&format!("refs/heads/{branch_name}"))
      .map_err(|e| cli_err!(Git, "Failed to switch to branch: {e}"))?;

    let db = database::load(&repo);

    if let Ok(mut db) = db {
      db.insert(branch_name, base);

      if database::save(&repo, db).is_err() {
        eprintln!("Failed to save branch data to database");
      };
    } else {
      eprintln!("Failed to load database. Couldn't save branch data")
    }

    Ok(())
  }
}
