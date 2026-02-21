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
      Action::Graph { interactive, pager } => self.graph(*interactive, pager),
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

      if branch_name == "main" || branch_name == "master" {
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

    // whether at least 1 proc failed
    let mut proc_failed = false;

    // await each process and check status
    for mut proc_info in children {
      if let Some(mut child) = proc_info.proc
        && await_child!(child, "Failed to delete branch").is_err()
      {
        // if proc failed
        proc_info.status = Status::FailedExec;
        proc_failed = true;
        println!("Failed to delete branch {}", proc_info.branch);
      };
    }

    if proc_failed {
      // if at least 1 proc failed
      Err(CliError::SubprocessFailed(
        "Failed to delete all merged branches".to_string(),
      ))
    } else {
      Ok(())
    }
  }

  fn list(&self) -> CliResult {
    let mut child = git!("branch", "-vv")?;
    await_child!(child, "Failed to call git")
  }

  fn log(&self) -> CliResult {
    let mut child = git!("log", "--oneline", "--decorate", "--all")?;
    await_child!(child, "Failed to call git")
  }

  fn graph(&self, interactive: bool, pager: &str) -> CliResult {
    let mut children: Vec<Child> = Vec::new();

    if interactive {
      // get git log output
      let mut git_graph = Command::new("git")
        .args(["log", "--graph", "--oneline", "--decorate", "--all"])
        .stdout(Stdio::piped())
        .spawn()?;

      let graph_stdout = git_graph.stdout.take().ok_or(CliError::SubprocessFailed(
        "Failed to get git log output".to_string(),
      ))?;

      // pipe into less to view interactively
      let pager = Command::new(pager).stdin(graph_stdout).spawn()?;

      children.push(pager);
      children.push(git_graph);
    } else {
      let child = git!("log", "--graph", "--oneline", "--decorate", "--all")?;
      children.push(child);
    }

    // await all procs
    for mut child in children {
      child.wait()?;
    }

    Ok(())
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
