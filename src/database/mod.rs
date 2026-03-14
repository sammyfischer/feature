//! Code to interact with the local feature database.
//!
//! The feature database is a simple text file in the project directory, much like `.git`. It keeps
//! track of feature branches and thier bases. This information is used to automatically rebase and
//! perform checks that require knowledge of a base branch.
//!
//! The format of the database is a sequence of lines that look like:
//! ```
//! branch base
//! ```
//! where `branch` and `base` are the names of a branch and its base, respectively. In other words,
//! each line represents an entry in the database, and fields are separated by spaces. If feature
//! fails to parse a line, it skips it an emits an error message. Since it's a simple text file, the
//! user can most likely fix the issue easily, but I'm considering adding commands to work with the
//! database more easily (e.g. `feature db sanitize` to delete malformed lines).
//!
//! The database can become out of date when the user uses git commands directly to create branches.
//! In that case, the cli will tell the user when it tries to find a base branch but can't. The user
//! can simply manually set the base branch with a dedicated command (e.g. `feature base`) or with
//! an option in the command that failed (e.g. `feature update --base <base>`). These commands will
//! keep the database as up to date as possible.
//!
//! The implication is that the database is very malleable and can be deleted and recreated, even if
//! that process is a bit tedious. It's simply a tool to speed up the workflow, and feature doesn't
//! depend on the database being well-formed.
//!
//! This is the best solution I could come up with. There aren't any reliable ways to automatically
//! detect base branches, though it is possible. This solution is also intuitive. The way I work
//! with git is much the same, I keep track of which branch corresponds to which base in my head,
//! then use that in git arguments accordingly. Since the user has to keep track of that information
//! regardless, I decided that this was the best solution.
//!
//! As a bonus, this database file can be easily hidden away inside the .git directory since it's
//! intrinsically linked to the repository.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::cli::CliResult;
use crate::cli::error::CliError;

pub type BranchMap = HashMap<String, String>;

/// Gets the path to the database file. Errors if the .git dir doesn't exist
pub fn path() -> CliResult<PathBuf> {
  let mut path = PathBuf::new();
  path.push(".git");
  if !path.exists() {
    return Err(CliError::Generic("Not a git repository!".into()));
  }
  path.push("feature");
  Ok(path)
}

/// Loads the map from the database file if it exists, else loads an empty map
pub fn load() -> CliResult<BranchMap> {
  let mut map: HashMap<String, String> = HashMap::new();
  let path = path()?;

  // if no file exists, return a blank map
  if !path.exists() {
    return Ok(map);
  }

  let text = fs::read_to_string(path)
    .map_err(|_| CliError::Database("Failed to read database file".into()))?;

  for (i, line) in text.lines().enumerate() {
    let mut parts = line.split(" ");

    let Some(branch) = parts.next() else {
      eprintln!("Error parsing line in database file:");
      eprintln!("{} | {}", i, line);
      continue;
    };

    let Some(base) = parts.next() else {
      eprintln!("Error parsing line in database file:");
      eprintln!("{} | {}", i, line);
      continue;
    };

    map.insert(branch.to_string(), base.to_string());
  }

  Ok(map)
}

/// Creates or overwrites the database file with the given data
pub fn save(database: BranchMap) -> CliResult {
  let path = path()?;

  let mut lines: Vec<String> = Vec::new();

  for (branch, base) in database.iter() {
    lines.push(format!("{} {}", branch, base));
  }

  fs::write(path, lines.join("\n"))?;
  Ok(())
}
