use std::fmt::Write;
use std::fs;

use git2::{ErrorCode, Rebase, Repository};

use crate::cli::{CliResult, get_current_branch};
use crate::{cli_err, cli_err_fn, data};

const MERGE_CONFLICT_MSG: &str = r"Merge conflict encountered! To resolve, do the following:
1. Edit the files to resolve conflicts
2. `git add <file>` for each resolved file
3. Either create a new commit with the changes, or amend the previous commit
4. `feature update -c` to continue the rebase

Alternatively, you can:
`feature update -a` to abort the entire rebase
`feature update -s` to skip the conflicting commit and continue";

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
  pub fn run(&self) -> CliResult {
    let repo = Repository::open_from_env()?;

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
      return Err(cli_err!(
        Generic,
        "A rebase is already in progress. Run again with -c to continue it, or -a to abort it"
      ));
    }

    let config = data::git_config(&repo)?;
    let branch_name = get_current_branch(&repo)?;

    let base_name = match &self.base {
      Some(it) => it.clone(),
      None => data::get_feature_base(&config, &branch_name)
        .ok_or(cli_err!(
          Database,
          "Failed to determine base branch. Manually specify it in this command or
          use `feature base <base>`",
        ))?
        .clone(),
    };

    if self.dry_run {
      println!("Using base: {}", base_name);
      return Ok(());
    }

    let base = repo.revparse_single(&base_name).map_err(cli_err_fn!(
      Git,
      e,
      "Failed to get base branch info for {}: {}",
      base_name,
      e
    ))?;

    let base_commit = repo
      .find_annotated_commit(
        base
          .peel_to_commit()
          .map_err(cli_err_fn!(
            Git,
            e,
            "Failed to find commit at {}: {}",
            base_name,
            e
          ))?
          .id(),
      )
      .map_err(cli_err_fn!(
        Git,
        e,
        "Failed to find commit at {}: {}",
        base_name,
        e
      ))?;

    let mut rebase = repo
      .rebase(None, Some(&base_commit), None, None)
      .map_err(cli_err_fn!(Git, e, "Failed to initiate rebase: {e}"))?;

    self.rebase(&repo, &mut rebase)
  }

  /// Runs the given rebase until it finishes or encounters a conflict
  fn rebase(&self, repo: &Repository, rebase: &mut Rebase) -> CliResult {
    while let Some(op) = rebase.next() {
      let _op = match op {
        Ok(it) => it,
        // Err(e) if e.code() == ErrorCode::MergeConflict => {
        //   println!("{}", MERGE_CONFLICT_MSG);
        //   self.dump_rebase(repo, rebase)?;
        //   return Ok(());
        // }
        Err(e) => return Err(cli_err!(Git, "Unknown error when rebasing: {e}")),
      };

      let index =
        repo
          .index()
          .map_err(cli_err_fn!(Git, e, "Failed to get the current index: {e}"))?;

      if index.has_conflicts() {
        println!("{}", MERGE_CONFLICT_MSG);
        self.dump_rebase(repo, rebase)?;
        return Ok(());
      }

      let signature = repo.signature().map_err(cli_err_fn!(
        Git,
        e,
        "Failed to get default commit signature: {e}"
      ))?;

      rebase.commit(None, &signature, None).map_err(cli_err_fn!(
        Git,
        e,
        "Error when applying commit. You can manually finish this rebase or
        `feature update -a` to quit it.
        Error: {e}"
      ))?;
    }

    rebase
      .finish(None)
      .map_err(cli_err_fn!(Git, e, "Failed to complete rebase: {e}"))
  }

  /// Opens and continues an existing rebase until it finishes or there's a conflict
  fn rebase_continue(&self, repo: &Repository) -> CliResult {
    if self.dry_run {
      if self.is_rebase_active(repo)? {
        println!("There is an active rebase, this command would continue it")
      } else {
        println!("There is no active rebase, this command would fail")
      }
      return Ok(());
    }

    let mut rebase =
      repo
        .open_rebase(None)
        .map_err(cli_err_fn!(Git, e, "Failed to open rebase: {e}"))?;

    self.rebase(repo, &mut rebase)
  }

  fn rebase_skip(&self, repo: &Repository) -> CliResult {
    if self.dry_run {
      if self.is_rebase_active(repo)? {
        println!("There is an active rebase, this command would skip the current commit")
      } else {
        println!("There is no active rebase, this command would fail")
      }

      return Ok(());
    }

    let mut rebase =
      repo
        .open_rebase(None)
        .map_err(cli_err_fn!(Git, e, "Failed to open rebase: {e}"))?;

    // call next once to skip, forward any errors
    if let Some(op) = rebase.next()
      && let Err(e) = op
      && e.code() != ErrorCode::Conflict
    {
      // propagate errors
      return Err(cli_err!(Git, "Unknown error when rebasing: {e}"));
    };

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
  fn rebase_abort(&self, repo: &Repository) -> CliResult {
    if self.dry_run {
      if self.is_rebase_active(repo)? {
        println!("There is an active rebase, this command would abort it")
      } else {
        println!("There is no active rebase, this command would fail")
      }
      return Ok(());
    }

    let mut rebase =
      repo
        .open_rebase(None)
        .map_err(cli_err_fn!(Git, e, "Failed to open rebase: {e}"))?;

    rebase
      .abort()
      .map_err(cli_err_fn!(Git, e, "Failed to abort rebase: {e}"))
  }

  fn is_rebase_active(&self, repo: &Repository) -> CliResult<bool> {
    match repo.open_rebase(None) {
      Ok(_) => Ok(true),
      Err(e) if e.code() == ErrorCode::NotFound => Ok(false),
      Err(e) => Err(cli_err!(Git, "Failed to check for active rebase: {e}")),
    }
  }

  /// Dumps remaining rebase steps into the git-rebase-todo. Allows the user to perform a `git
  /// rebase --skip` after, which relies on this file.
  fn dump_rebase(&self, repo: &Repository, rebase: &mut Rebase) -> CliResult {
    let current = rebase
      .operation_current()
      .ok_or(cli_err!(Git, "No current rebase operation"))?;

    let total = rebase.len();
    let mut buf = String::new();

    for i in (current + 1)..total {
      let op = rebase.nth(i).ok_or(cli_err!(
        Git,
        "Failed to find rebase operation number {}",
        i
      ))?;

      // commit id
      let id = op.id();

      // rebase operations are pick by default
      writeln!(buf, "pick {}", id).map_err(cli_err_fn!(
        Git,
        e,
        "Failed to dump rebase state: {e}"
      ))?;
    }

    let rebase_data_dir = repo.path().join("rebase-merge");

    // git uses the git-rebase-todo file to continue an existing rebase
    let path = rebase_data_dir.join("git-rebase-todo");
    fs::write(&path, &buf).map_err(cli_err_fn!(Git, e, "Failed to dump rebase state: {e}"))?;

    // libgit2 uses a file called current which just stores the current oid
    let id = rebase
      .nth(current)
      .ok_or(cli_err!(Git, "Failed to get current rebase operation"))?
      .id()
      .to_string();
    fs::write(rebase_data_dir.join("current"), id)?;

    Ok(())
  }
}
