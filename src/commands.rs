use std::io::Read;
use std::process::{Child, Command, Stdio};

use crate::cli_error::{CliError, CliResult};
use crate::validate_branch_name;

/// Waits on the child process, returns result
macro_rules! await_child {
  ($child:expr) => {
    match $child.wait() {
      Ok(ok) if ok.success() => Ok(()),
      _ => Err(CliError::GitProcFailed),
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

/// Start a new feature branch with remaining arguments as branch name
pub fn start(words: &[String]) -> CliResult {
  let branch_name = words.join("-");
  validate_branch_name(&branch_name)?;
  println!("Creating branch: {}", branch_name);

  let mut child = git!("switch", "-c", branch_name)?;
  await_child!(child)
}

/// Commit with remaining arguments as commit message
pub fn commit(words: &[String]) -> CliResult {
  let commit_msg = words.join(" ");
  println!("Committing with message: {}", commit_msg);

  let mut child = git!("commit", "-m", commit_msg)?;
  await_child!(child)
}

/// Update current branch with base branch (using rebase)
pub fn update() -> CliResult {
  println!("Updating against base");
  Ok(())
}

/// Rebase current branch onto base branch
pub fn merge() -> CliResult {
  println!("Merging into base");
  Ok(())
}

pub fn prune() -> CliResult {
  // get list of merged branches
  let child = git!("branch", "--merged")?;
  let mut stdout = child.stdout.ok_or(CliError::GitProcFailed)?;

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

  let mut children: Vec<ProcInfo> = Vec::new();
  for line in output.lines() {
    // clean up line to just get the branch name
    let branch_name = line.trim_prefix("* ").trim();

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

  // whether at least 1 proc failed
  let mut proc_failed = false;

  // await each process and check status
  for mut proc_info in children {
    if let Some(mut child) = proc_info.proc
      && await_child!(child).is_err()
    {
      // if proc failed
      proc_info.status = Status::FailedExec;
      proc_failed = true;
      println!("Failed to delete branch {}", proc_info.branch);
    };
  }

  if proc_failed {
    // if at least 1 proc failed
    Err(CliError::GitProcFailed)
  } else {
    Ok(())
  }
}

pub fn list() -> CliResult {
  let mut child = git!("branch", "-vv")?;
  await_child!(child)
}

pub fn log() -> CliResult {
  let mut child = git!("log", "--oneline", "--decorate", "--all")?;
  await_child!(child)
}

pub fn graph(interactive: bool, pager: &str) -> CliResult {
  let mut children: Vec<Child> = Vec::new();

  if interactive {
    // get git log output
    let mut git_graph = Command::new("git")
      .args(["log", "--graph", "--oneline", "--decorate", "--all"])
      .stdout(Stdio::piped())
      .spawn()?;

    let graph_stdout = git_graph.stdout.take().ok_or(CliError::GitProcFailed)?;

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
