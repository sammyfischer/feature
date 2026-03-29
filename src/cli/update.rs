use std::fmt::Write;
use std::fs;

use anyhow::{Context, Result, anyhow};
use git2::{ErrorCode, Rebase, Repository};

use crate::cli::get_current_branch;
use crate::{data, open_repo};

const MERGE_CONFLICT_MSG: &str = r"Merge conflict encountered! To resolve, do the following:

1. Edit the files to resolve conflicts
2. `git add <file>` for each resolved file
3. Either create a new commit with the changes, or amend the previous commit
4. `feature update -c` to continue the rebase

Alternatively, you can:
`feature update -a` to abort the entire rebase
`feature update -s` to skip the conflicting commit and continue";

const NO_BASE_MSG: &str = r"No base branch found. You can either:

Manually specify the base branch: `feature update <BASE_BRANCH>`
Set the base branch permanently: `feature base <BASE_BRANCH>`";

const COMMIT_FAILED_MSG: &str = r"Failed to apply commit. You can:

`feature update -c` the rebase to try continuing
`feature update -s` to skip applying this commit
`feature update -a` to abort the rebase";

const NO_SIGNATURE_MSG: &str = r"Failed to get default commit signature. Try setting them in your git config:

`git config user.name <name>`
`git config user.email <email>`";

#[derive(clap::Args, Clone, Debug)]
pub struct Args {
  /// Output which base branch will be used, but don't perform the rebase or modify the database.
  #[arg(long)]
  dry_run: bool,

  /// Continue an active rebase
  #[arg(short, long)]
  r#continue: bool,

  /// Skip current patch
  #[arg(short, long)]
  skip: bool,

  /// Abort an active rebase
  #[arg(short, long)]
  abort: bool,

  /// The name of the base branch to use.
  #[arg(value_name = "BASE_BRANCH")]
  base: Option<String>,
}

impl Args {
  pub fn run(&self) -> Result<()> {
    let repo = open_repo!();

    if self.r#continue {
      return self.rebase_continue(&repo);
    }

    if self.skip {
      return self.rebase_skip(&repo);
    }

    if self.abort {
      return self.rebase_abort(&repo);
    }

    // fail if there's already an active rebase
    if self.is_rebase_active(&repo)? {
      return Err(anyhow!("A rebase is already in progress"));
    }

    let config = data::git_config(&repo)?;
    let branch_name = get_current_branch(&repo)?;

    let base_name = match &self.base {
      Some(it) => it.clone(),
      None => data::get_feature_base(&config, &branch_name)
        .ok_or(anyhow!(NO_BASE_MSG))?
        .clone(),
    };

    if self.dry_run {
      println!("Using base: {}", base_name);
      return Ok(());
    }

    // error instead of panic, base name could be invalid
    let base = repo
      .revparse_single(&base_name)
      .with_context(|| format!("Failed to get reference to base branch: {}", base_name))?;

    let base_commit = repo
      .find_annotated_commit(
        base
          .peel_to_commit()
          .with_context(|| format!("Failed to find commit pointed to by {}", base_name))?
          .id(),
      )
      .with_context(|| format!("Failed to find commit pointed to by {}", base_name))?;

    let mut rebase = repo
      .rebase(None, Some(&base_commit), None, None)
      .expect("Failed to initiate rebase");

    self.rebase(&repo, &mut rebase)
  }

  /// Runs the given rebase until it finishes or encounters a conflict
  fn rebase(&self, repo: &Repository, rebase: &mut Rebase) -> Result<()> {
    while let Some(op) = rebase.next() {
      op.context("Failed to get next rebase operation")?;

      let index = repo
        .index()
        .expect("Failed to get index to build rebase commit on");

      if index.has_conflicts() {
        println!("{}", MERGE_CONFLICT_MSG);
        self.dump_rebase(repo, rebase)?;
        return Ok(());
      }

      let signature = repo.signature().context(NO_SIGNATURE_MSG)?;

      rebase
        .commit(None, &signature, None)
        .expect(COMMIT_FAILED_MSG);
    }

    rebase.finish(None).expect("Failed to finish rebase");
    Ok(())
  }

  /// Opens and continues an existing rebase until it finishes or there's a conflict
  fn rebase_continue(&self, repo: &Repository) -> Result<()> {
    if self.dry_run {
      if self.is_rebase_active(repo)? {
        println!("There is an active rebase, this command would continue it")
      } else {
        println!("There is no active rebase, this command would fail")
      }
      return Ok(());
    }

    let mut rebase = repo.open_rebase(None).expect("Failed to open rebase");

    self.rebase(repo, &mut rebase)
  }

  fn rebase_skip(&self, repo: &Repository) -> Result<()> {
    if self.dry_run {
      if self.is_rebase_active(repo)? {
        println!("There is an active rebase, this command would skip the current commit")
      } else {
        println!("There is no active rebase, this command would fail")
      }

      return Ok(());
    }

    let mut rebase = repo.open_rebase(None).expect("Failed to open rebase");

    // call next once to skip, forward any errors
    if let Some(op) = rebase.next()
      && let Err(e) = op
      && e.code() != ErrorCode::Conflict
    {
      // propagate errors
      return Err(anyhow!("Unknown error when rebasing: {}", e));
    };

    // for debugging, remove when skip is fixed:
    // println!("Remaining operations:");
    // for i in rebase.operation_current().unwrap()..rebase.len() {
    //   let Some(op) = rebase.nth(i) else {
    //     continue;
    //   };
    //   let cmd = match op.kind().unwrap() {
    //     git2::RebaseOperationType::Pick => "pick",
    //     git2::RebaseOperationType::Reword => "reword",
    //     git2::RebaseOperationType::Edit => "edit",
    //     git2::RebaseOperationType::Squash => "squash",
    //     git2::RebaseOperationType::Fixup => "fixup",
    //     git2::RebaseOperationType::Exec => "exec",
    //   };
    //   println!("{} {}", cmd, op.id());
    // }

    // finish the rebase
    self.rebase(repo, &mut rebase)?;

    // if no errors, exit successfully
    Ok(())
  }

  /// Opens and aborts an existing rebase
  fn rebase_abort(&self, repo: &Repository) -> Result<()> {
    if self.dry_run {
      if self.is_rebase_active(repo)? {
        println!("There is an active rebase, this command would abort it")
      } else {
        println!("There is no active rebase, this command would fail")
      }
      return Ok(());
    }

    let mut rebase = repo.open_rebase(None).expect("Failed to open rebase");

    rebase.abort().expect("Failed to abort rebase");
    Ok(())
  }

  /// Whether a rebase is currently active. Panics if there's an unknown error
  fn is_rebase_active(&self, repo: &Repository) -> Result<bool> {
    match repo.open_rebase(None) {
      Ok(_) => Ok(true),
      Err(e) if e.code() == ErrorCode::NotFound => Ok(false),
      Err(e) => panic!("Failed to check for active rebase: {}", e),
    }
  }

  /// Dumps remaining rebase steps into the git-rebase-todo. Allows the user to perform a `git
  /// rebase --skip` after, which relies on this file.
  fn dump_rebase(&self, repo: &Repository, rebase: &mut Rebase) -> Result<()> {
    let current = rebase
      .operation_current()
      .expect("No current rebase operation");

    let total = rebase.len();
    let mut buf = String::new();

    for i in (current + 1)..total {
      let op = rebase
        .nth(i)
        .unwrap_or_else(|| panic!("Failed to find rebase operation number {}", i));

      // commit id
      let id = op.id();

      // rebase operations are pick by default
      writeln!(buf, "pick {}", id)
        .unwrap_or_else(|_| panic!("Failed to write rebase operation {}", i));
    }

    let rebase_data_dir = repo.path().join("rebase-merge");

    // git uses the git-rebase-todo file to continue an existing rebase
    let path = rebase_data_dir.join("git-rebase-todo");
    fs::write(&path, &buf).expect("Failed to write remaining operations to file");

    // libgit2 uses a file called current which just stores the current oid
    let id = rebase
      .nth(current)
      .expect("Failed to get current rebase operation")
      .id()
      .to_string();

    fs::write(rebase_data_dir.join("current"), id)
      .expect("Failed to write current rebase operation to file");

    Ok(())
  }
}
