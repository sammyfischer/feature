//! Useful functions, macros, and core command implementation

use std::io::Read;
use std::process::{Child, Command, Stdio};

use clap::Parser;

use crate::cli::def::{Action, Args, Cli, ConfigCmd, StartArgs};
use crate::cli::errors::CliError;
use crate::config::{self, Config};

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
      Action::Start(args) => self.start(args),
      Action::Commit { words } => self.commit(words),
      Action::Update => self.update(),
      Action::Merge => self.merge(),
      Action::Protect { branch } => self.protect(branch.clone()),
      Action::Unprotect { branch } => self.unprotect(branch.clone()),
      Action::Prune { dry_run } => self.prune(*dry_run),
      Action::List => self.list(),
      Action::Log => self.log(),
      Action::Graph => self.graph(),
      Action::Config { args } => self.config(&args.clone()),
    }
  }

  /// Start a new feature branch with remaining arguments as branch name
  fn start(&self, args: &StartArgs) -> CliResult {
    let sep = args.sep.as_ref().unwrap_or(&self.config.branch_sep);

    let branch_name = args.words.join(sep);
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
    let mut doc = config::read_doc()?;
    let branches = doc["protected_branches"].as_array();

    let mut branches = if let Some(it) = branches {
      it.clone()
    } else {
      toml_edit::Array::new()
    };

    branches.push(branch);
    doc["protected_branches"] = toml_edit::value(branches);

    config::write(&doc)?;
    Ok(())
  }

  fn unprotect(&mut self, branch: String) -> CliResult {
    let mut doc = config::read_doc()?;

    // if protected_branches is None, then there's nothing to remove, no need to error
    let Some(branches) = doc["protected_branches"].as_array() else {
      return Ok(());
    };

    let mut branches = branches.clone();

    // find index
    let i = branches.iter().position(|b| b.as_str() == Some(&branch));

    // modify if found, else leave untouched
    if let Some(i) = i {
      branches.remove(i);
      doc["protected_branches"] = toml_edit::value(branches);
      config::write(&doc)?;
    }

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
    // git pretty format:
    // %h = hash, %d = decorator (e.g. branch pointing to that commit)
    // %s = subject (commit description title line)
    // %an = author name, %ar = author date (relative)
    let mut child = git!(
      "log",
      "--all",
      "--pretty=format:%C(auto)%h%d %C(reset)%s %C(dim)(%an, %ar)"
    )?;
    await_child!(child, "Failed to call git")
  }

  fn graph(&self) -> CliResult {
    // like log, but author name and date are first, and message is truncated to try and fit in one
    // line
    let mut child = git!(
      "log",
      "--graph",
      "--all",
      "--pretty=format:%C(auto)%h%d %C(green)%an %C(blue)%ar %C(reset)%<(50,trunc)%s"
    )?;
    await_child!(child, "Failed to call git")
  }

  fn config(&mut self, cmd: &ConfigCmd) -> CliResult {
    match cmd {
      ConfigCmd::Set(args) => {
        let mut doc = config::read_doc()?;

        if let Some(it) = &args.branch_sep {
          doc["branch_sep"] = toml_edit::value(it);
        }

        config::write(&doc)?;
      }
    };

    Ok(())
  }
}
