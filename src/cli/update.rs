use std::fmt::Write;
use std::fs;

use anyhow::{Context, Result, anyhow};
use console::style;
use git2::{ErrorCode, Rebase, Repository};

use crate::util::advice::{NO_SIGNATURE_MSG, REBASE_CONFLICT_ADVICE};
use crate::util::branch::get_current_branch_name;
use crate::util::diff::DiffSummary;
use crate::util::display::display_hash;
use crate::util::get_current_commit;
use crate::{App, data};

const LONG_ABOUT: &str = r"Rebases this branch onto its base. The available commands are similar to a git
rebase.";

const NO_BASE_MSG: &str = r#"No base branch found. You can:

• Manually specify the base branch: "feature update <BASE_BRANCH>"
• Set the base branch permanently: "feature base <BASE_BRANCH>""#;

const COMMIT_FAILED_MSG: &str = r#"Failed to apply commit. You can:

• Try to continue with "git rebase --continue"
• Skip applying the current commit with "git rebase --skip"
• Abort the rebase with "git rebase --abort""#;

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Updates this branch with its base", long_about = LONG_ABOUT)]
pub struct Args {
  /// Output which base branch will be used, but don't perform the rebase or modify the database.
  #[arg(long)]
  dry_run: bool,

  /// Continue an active rebase
  #[arg(short, long)]
  r#continue: bool,

  /// Abort an active rebase
  #[arg(short, long)]
  abort: bool,

  /// The name of the base branch to use.
  base: Option<String>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    if self.r#continue {
      return self.rebase_continue(&state.repo);
    }

    if self.abort {
      return self.rebase_abort(&state.repo);
    }

    // fail if there's already an active rebase
    if self.is_rebase_active(&state.repo)? {
      return Err(anyhow!("A rebase is already in progress"));
    }

    let config = data::git_config(&state.repo)?;
    let branch_name = get_current_branch_name(&state.repo)?
      .context("Not currently on a branch! Nothing to update.")?;

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
    let base = state
      .repo
      .revparse_single(&base_name)
      .with_context(|| format!("Failed to get reference to base branch: {}", base_name))?;

    let base_commit = state
      .repo
      .find_annotated_commit(
        base
          .peel_to_commit()
          .with_context(|| format!("Failed to find commit pointed to by {}", base_name))?
          .id(),
      )
      .with_context(|| format!("Failed to find commit pointed to by {}", base_name))?;

    let mut rebase = state
      .repo
      .rebase(None, Some(&base_commit), None, None)
      .context("Failed to initiate rebase")?;

    self.rebase(&state.repo, &mut rebase)?;

    println!(
      "{} {} with changes from {}",
      style("Updated").green(),
      style(branch_name).blue(),
      style(
        base_name
          .trim_prefix("refs/remotes/")
          .trim_prefix("refs/heads/")
      )
      .magenta()
    );
    Ok(())
  }

  /// Runs the given rebase until it finishes or encounters a conflict
  fn rebase(&self, repo: &Repository, rebase: &mut Rebase) -> Result<()> {
    while let Some(op) = rebase.next() {
      let id = op.context("Failed to get next rebase operation")?.id();

      let index = repo
        .index()
        .context("Failed to get index to build rebase commit on")?;

      if index.has_conflicts() {
        let commit = get_current_commit(repo)?;
        match commit {
          Some(commit) => {
            let tree = commit.tree()?;
            let diff = repo.diff_tree_to_index(Some(&tree), Some(&index), None)?;
            let summary = DiffSummary::new(&diff)?;

            eprintln!("{}", REBASE_CONFLICT_ADVICE);

            println!(
              "\n{} - {}",
              style("Conflicts").yellow(),
              if summary.num_files != 0 {
                summary.display_conflicts()
              } else {
                style("none").green().to_string()
              }
            );
          }
          None => println!("Failed to display conflicts"),
        }
        self.dump_rebase(repo, rebase)?;
        return Err(anyhow!("Rebase conflicts"));
      }

      let signature = repo.signature().context(NO_SIGNATURE_MSG)?;

      rebase
        .commit(None, &signature, None)
        .context(COMMIT_FAILED_MSG)?;

      println!("{} commit {}", style("Applied").green(), display_hash(&id));
    }

    rebase.finish(None).context("Failed to finish rebase")?;
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

    let mut rebase = repo.open_rebase(None).context("Failed to open rebase")?;
    self.rebase(repo, &mut rebase)
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

    let mut rebase = repo.open_rebase(None).context("Failed to open rebase")?;
    rebase.abort().context("Failed to abort rebase")?;
    println!("{} rebase", style("Aborted").green());
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

  /// Dumps remaining rebase steps into the git-rebase-todo. Allows the user to use native git
  /// rebase commands.
  fn dump_rebase(&self, repo: &Repository, rebase: &mut Rebase) -> Result<()> {
    let current = rebase
      .operation_current()
      .expect("There should be a current rebase operation");

    let total = rebase.len();
    // always 40 char hash, some extra space for the operation. there will always be at least one
    // line
    let mut buf = String::with_capacity(50);

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
    fs::write(&path, &buf).context("Failed to write remaining operations to file")?;

    // libgit2 uses a file called current which just stores the current oid
    let id = rebase
      .nth(current)
      .expect("There should be a current rebase operation")
      .id()
      .to_string();

    fs::write(rebase_data_dir.join("current"), id)
      .context("Failed to write current rebase operation to file")?;

    Ok(())
  }
}
