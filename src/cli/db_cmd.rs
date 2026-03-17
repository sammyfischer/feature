//! Database subcommand

use std::fs;

use clap::Subcommand;
use git2::Repository;

use super::error::CliError;
use crate::cli::{CliResult, get_current_branch, get_user_confirmation};
use crate::database;

#[derive(Clone, Debug, Subcommand)]
pub enum Args {
  /// Track a branch and its base
  Add {
    /// The base branch
    base: String,

    /// The branch whose base will be set. Defaults to current branch
    branch: Option<String>,
  },

  /// Remove a tracked branch from the database
  #[command(visible_alias = "rm")]
  Remove { branch: String },

  /// Clean all deleted branches from database
  Clean,

  /// Removes the entire database
  Delete {
    /// Delete without confirmation
    #[arg(short, long)]
    force: bool,
  },
}

impl Args {
  pub fn run(&self) -> CliResult {
    match self {
      Args::Add { base, branch } => self.add(base, branch),
      Args::Remove { branch } => self.remove(branch),
      Args::Clean => self.clean(),
      Args::Delete { force } => self.delete(*force),
    }
  }

  fn add(&self, base: &String, branch: &Option<String>) -> CliResult {
    let repo = Repository::open_from_env()?;
    let branch = match branch {
      Some(it) => it,
      None => {
        let repo = Repository::open_from_env()?;
        &get_current_branch(&repo)?
      }
    };

    let mut db = database::load(&repo)?;
    db.insert(branch.to_string(), base.to_string());
    database::save(&repo, db)
  }

  fn remove(&self, branch: &String) -> CliResult {
    let repo = Repository::open_from_env()?;
    let mut db = database::load(&repo)?;
    db.remove(branch);
    database::save(&repo, db)
  }

  fn clean(&self) -> CliResult {
    let repo = Repository::open_from_env()?;
    let mut db = database::load(&repo)?;
    database::clean(&mut db);
    database::save(&repo, db)
  }

  fn delete(&self, force: bool) -> CliResult {
    let repo = Repository::open_from_env()?;
    let path = database::path(&repo);
    if !path.exists() {
      println!("Database does not exist");
      return Ok(());
    }

    let should_delete = force
      || get_user_confirmation(
        "Are you sure you want to delete the database? This is irreversable (but honestly it's not that hard to remake it)",
      )?;

    if should_delete {
      fs::remove_file(path)
        .map_err(|e| CliError::Database(format!("Failed to delete database file: {}", e)))
    } else {
      Ok(())
    }
  }
}
