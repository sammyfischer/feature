//! Useful functions, macros, and core command implementation

use std::io::Read;
use std::process::{Child, Command, Stdio};

use clap::Parser;

use crate::cli::def::{Action, Args, Cli, ConfigCmd};
use crate::cli::errors::CliError;
use crate::config::{Config, write_config};

pub mod def;
mod errors;

/// Waits on the child process, returns result
macro_rules! await_child {
  ($child:expr, $msg:expr) => {
    if $child.wait().is_ok_and(|status| status.success()) {
      Ok(())
    } else {
      Err(CliError::SubprocessFailed($msg.to_string()))
    }
  };
}

/// Spawns a git command, passing this macros args as command line args
macro_rules! git {
  ($($arg:tt),* $(,)?) => {
    {
      let mut cmd = std::process::Command::new("git");
      $(
        cmd.arg($arg);
      )*
      cmd.spawn()
    }
  };
}

/// Returns an args `Some` value, else gets the value from `self.config`.
///
/// # Parameters
/// - `self` - the `self` instance, which must have the `config` field
/// - `args` - a struct containing args to use. Must have a field with the same name as `opt` and be
///   an `Option`
/// - `opt` - the name of the config option field, defined on `self.config` and `args`. Should be
///   written as an identifier. Supports borrowing with the usual borrow operator
macro_rules! default_to_config {
  ($self:ident, $args:expr, $opt:ident) => {
    match $args.$opt {
      Some(it) => it,
      None => $self.config.$opt,
    }
  };

  // separate rule, borrow operator needs to be handled differently
  ($self:ident, $args:expr, & $opt:ident) => {
    match &$args.$opt {
      Some(it) => it,
      None => &$self.config.$opt,
    }
  };
}

pub type CliResult<T = ()> = Result<T, CliError>;

impl Cli {
  pub fn new(config: Config) -> Self {
    let args = Args::parse();
    Self { config, args }
  }

  pub fn run(&mut self) -> CliResult {
    match &self.args.action {
      Action::Start { words } => self.start(words),
      Action::Commit { words } => self.commit(words),
      Action::Update => self.update(),
      Action::Merge => self.merge(),
      Action::Protect { branch } => self.protect(branch.clone()),
      Action::Unprotect { branch } => self.unprotect(branch.clone()),
      Action::Prune { dry_run } => self.prune(*dry_run),
      Action::List => self.list(),
      Action::Log => self.log(),
      Action::Graph(_) => self.graph(),
      Action::Config { args } => self.config(&args.clone()),
    }
  }

  /// Start a new feature branch with remaining arguments as branch name
  fn start(&self, words: &[String]) -> CliResult {
    let branch_name = words.join("-");
    validate_branch_name(&branch_name)?;
    println!("Creating branch: {}", branch_name);

    let mut child = git!("switch", "-c", branch_name)?;
    await_child!(child, "git failed to execute")
  }

  /// Commit with remaining arguments as commit message
  fn commit(&self, words: &[String]) -> CliResult {
    let commit_msg = words.join(" ");
    println!("Committing with message: {}", commit_msg);

    let mut child = git!("commit", "-m", commit_msg)?;
    await_child!(child, "git failed to execute")
  }

  /// Update current branch with base branch (using rebase)
  fn update(&self) -> CliResult {
    println!("Updating against base");
    Ok(())
  }

  /// Rebase current branch onto base branch
  fn merge(&self) -> CliResult {
    println!("Merging into base");
    Ok(())
  }

  fn protect(&mut self, branch: String) -> CliResult {
    self.config.protected_branches.push(branch);
    write_config(&self.config)?;
    Ok(())
  }

  fn unprotect(&mut self, branch: String) -> CliResult {
    if let Some(i) = self
      .config
      .protected_branches
      .iter()
      .position(|b| *b == branch)
    {
      self.config.protected_branches.remove(i);
    };

    write_config(&self.config)?;
    Ok(())
  }

  fn prune(&self, dry_run: bool) -> CliResult {
    // get list of merged branches
    let child = Command::new("git")
      .args(["branch", "--merged"])
      .stdout(Stdio::piped())
      .spawn()?;
    let mut stdout = child.stdout.ok_or(CliError::Generic(
      "Failed to get merged branches".to_string(),
    ))?;

    let mut output = String::new();
    stdout.read_to_string(&mut output)?;

    // current state of the process (might use this for better error messages later)
    enum Status {
      Started,
      FailedStart,
      FailedExec,
    }

    // metadata about each process (and the process itself)
    struct ProcInfo<'branch> {
      status: Status,
      proc: Option<Child>,
      branch: &'branch str,
    }

    if dry_run {
      println!("Deletion candidates:")
    }

    let mut children: Vec<ProcInfo> = Vec::new();
    for line in output.lines() {
      if line.starts_with("*") {
        // skip current branch
        continue;
      }

      // clean up line to just get the branch name
      let branch_name = line.trim();

      if self
        .config
        .protected_branches
        .contains(&branch_name.to_string())
      {
        // skip protected branches
        continue;
      }

      if dry_run {
        // print branch name, skip over branch deletion
        println!("{}", branch_name);
        continue;
      }

      // start process to delete branch
      let child = git!("branch", "-d", branch_name);
      let proc_info = if let Ok(child) = child {
        ProcInfo {
          status: Status::Started,
          proc: Some(child),
          branch: branch_name,
        }
      } else {
        ProcInfo {
          status: Status::FailedStart,
          proc: None,
          branch: branch_name,
        }
      };
      children.push(proc_info);
    }

    // exit early, all branch candidates have been printed
    if dry_run {
      return Ok(());
    }

    let mut result: CliResult = Ok(());

    // await each process and check status
    for mut proc_info in children {
      if let Some(mut child) = proc_info.proc
        && await_child!(child, "Failed to delete branch").is_err()
      {
        // if proc failed
        proc_info.status = Status::FailedExec;
        result = Err(CliError::SubprocessFailed(
          "Failed to delete all merged branches".to_string(),
        ));
        println!("Failed to delete branch {}", proc_info.branch);
      };
    }

    result
  }

  fn list(&self) -> CliResult {
    let mut child = git!("branch", "-vv")?;
    await_child!(child, "Failed to call git")
  }

  fn log(&self) -> CliResult {
    let mut child = git!("log", "--oneline", "--decorate", "--all")?;
    await_child!(child, "Failed to call git")
  }

  fn graph(&self) -> CliResult {
    let mut child = git!("log", "--graph", "--oneline", "--decorate", "--all")?;
    await_child!(child, "Failed to call git")
  }

  fn config(&mut self, args: &ConfigCmd) -> CliResult {
    match args {
      ConfigCmd::Set(args) => self.config.set(args),
    }?;

    write_config(&self.config)?;
    Ok(())
  }
}

/// Checks if a branch name is allowed. This is likely more strict than actual git rules for branch
/// names.
fn validate_branch_name(name: &str) -> CliResult {
  if name.contains(|c: char| !(c.is_alphanumeric() || c == '/' || c == '-')) {
    Err(CliError::BadBranchName(name.to_string()))
  } else {
    Ok(())
  }
}
