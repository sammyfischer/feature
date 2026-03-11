//! Useful functions, macros, and core command implementation

use std::io::Read;
use std::process::{Command, Stdio};

use clap::Parser;
use regex::Regex;
use unicode_width::UnicodeWidthChar;

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
  ($($arg:expr),* $(,)?) => {
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
  pub fn new() -> Self {
    let config = crate::config::read().unwrap_or_default();
    let args = Args::parse();
    Self { config, args }
  }

  pub fn run(&mut self) -> CliResult {
    match &self.args.action {
      Action::Start(args) => self.start(args),
      Action::Commit { words } => self.commit(words),
      Action::Update { base } => self.update(base),
      Action::Push => self.push(),
      Action::Protect { branch } => self.protect(branch.clone()),
      Action::Unprotect { branch } => self.unprotect(branch.clone()),
      Action::Sync => self.sync(),
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

    await_child!(git!("switch", "-c", branch_name)?, "git failed to execute")
  }

  /// Commit with remaining arguments as commit message
  fn commit(&self, words: &[String]) -> CliResult {
    let commit_msg = words.join(" ");
    println!("Committing with message: {}", commit_msg);

    await_child!(git!("commit", "-m", commit_msg)?, "git failed to execute")
  }

  /// Update current branch with base branch (using rebase)
  fn update(&self, base: &Option<String>) -> CliResult {
    let branch = match base {
      Some(it) => it,
      None => &self.config.base_branch,
    };

    await_child!(git!("rebase", branch)?, "Failed to call git")
  }

  fn push(&self) -> CliResult {
    let branch = get_current_branch()
      .ok_or_else(|| CliError::SubprocessFailed("Failed to get current branch name".to_string()))?;

    if self.config.protected_branches.contains(&branch) {
      eprintln!("This is a protected branch, refusing to push");
      return Ok(());
    }

    // -u = set upstream, doesn't hurt to do this every time
    // --force-with-lease = protects against overwriting others' work, but allows pushing after
    // rebasing with main (since that changes commit history)
    await_child!(
      git!("push", "-u", "--force-with-lease", &self.config.default_remote, branch)?,
      "git failed"
    )
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

  fn sync(&self) -> CliResult {
    fetch_all()?;

    // Outupt looks like: `main origin/main`. Branches w/o a remote won't match. Captures the local
    // branch name, remote name, and remote branch name, e.g. (local_branch origin/remote_branch).
    //
    // Unwrapping is fine here bc this should only error if the pattern doesn't parse, which needs
    // to be fixed immediately.
    let regex = Regex::new(r"(\w+) (\w+)/(\w+)").unwrap();

    // gets every branch and its remote tracking branch
    let output = String::from_utf8(
      Command::new("git")
        .args([
          "for-each-ref",
          "--format=%(refname:short) %(upstream:short)",
          "refs/heads",
        ])
        .output()?
        .stdout,
    )
    .map_err(|e| CliError::SubprocessFailed(format!("Git output error: {}", e)))?;

    for line in output.lines() {
      let Some(captures) = regex.captures(line) else {
        continue;
      };

      // local branch name match
      let local_branch = match captures.get(1) {
        Some(it) => it.as_str(),
        None => continue,
      };

      // remote branch name match
      let remote = match captures.get(2) {
        Some(it) => it.as_str(),
        None => continue,
      };

      // remote branch name match
      let remote_branch = match captures.get(3) {
        Some(it) => it.as_str(),
        None => continue,
      };

      // `git fetch origin main:main` gets latest origin/main and fast-forwards main to match
      // origin/main
      let Ok(mut child) = git!("fetch", remote, format!("{remote_branch}:{local_branch}")) else {
        continue;
      };
      let _ = await_child!(child, "git failed");
    }

    Ok(())
  }

  fn prune(&self, dry_run: bool) -> CliResult {
    fetch_all()?;

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

    if dry_run {
      println!("Deletion candidates:")
    }

    for line in output.lines() {
      if line.starts_with("*") {
        // skip current branch
        continue;
      }

      // clean up line to just get the branch name
      let branch_name = line.trim();

      // skip protected branches
      if self
        .config
        .protected_branches
        .contains(&branch_name.to_string())
      {
        continue;
      }

      if dry_run {
        // print branch name, skip over branch deletion
        println!("{}", branch_name);
        continue;
      }

      // delete and await 1 by 1 so stdout doesn't get interlaced with all the output
      let child = git!("branch", "-d", branch_name);
      if let Ok(mut child) = child {
        let _ = await_child!(child, format!("Failed to delete branch {}", branch_name));
      };
    }

    Ok(())
  }

  fn list(&self) -> CliResult {
    await_child!(git!("branch", "-vv")?, "Failed to call git")
  }

  fn log(&self) -> CliResult {
    // git pretty format:
    // %h = hash, %d = decorator (e.g. branch pointing to that commit)
    // %s = subject (commit description title line)
    // %an = author name, %ar = author date (relative)
    await_child!(
      git!(
        "log",
        "--all",
        "--pretty=format:%C(auto)%h%d %C(reset)%s %C(dim)(%an, %ar)"
      )?,
      "Failed to call git"
    )
  }

  fn graph(&self) -> CliResult {
    // like log, but author name and date are first, and message is truncated to try and fit in one
    // line
    let output = Command::new("git")
      .args([
        "log",
        "--graph",
        "--all",
        "--color=always", // git detects that output isn't terminal, so by default won't use color
        "--pretty=format:%C(auto)%h%d %C(green)%an %C(blue)%ar %C(reset)%s",
      ])
      .output()?;

    let string_output = String::from_utf8(output.stdout)?;
    let lines = string_output.lines();
    let mut out_lines: Vec<String> = Vec::new();
    let term_width = get_term_width();

    // truncate each line to term width
    for line in lines {
      let mut acc_width = 0usize;
      let mut line_buf = String::new();
      let mut escape_sequence = false;

      // push characters until term width is exceeded
      'chars: for c in line.chars() {
        // unicode-width counts ansi escape sequences as 1 wide. manually push and ignore width
        if c == '\x1b' {
          escape_sequence = true;
          line_buf.push(c);
          continue;
        }

        // keep handling ansi escape sequence until the end. in our case, we only deal with color
        // codes (and reset) which all end with 'm'
        if escape_sequence && c == 'm' {
          if c == 'm' {
            escape_sequence = false;
          }
          line_buf.push(c);
          continue;
        }

        let char_width = c.width().unwrap_or(0);

        if acc_width + char_width > term_width {
          break 'chars;
        }

        acc_width += char_width;
        line_buf.push(c);
      }

      // push line to output
      out_lines.push(line_buf);
    }

    // side-effect of manually printing is that we don't get automatic paging. we'll have to do that
    // manually and likely implement config for that
    println!("{}", out_lines.join("\n"));
    Ok(())
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

      ConfigCmd::Unset(args) => {
        let mut doc = config::read_doc()?;

        if args.branch_sep {
          let old_val = doc.remove_entry("branch_sep");
          if let Some((_, item)) = old_val {
            println!("Unset branch_sep (was {})", item);
          } else {
            println!("branch_sep is already unset");
          }
        }

        config::write(&doc)?;
      }
    };

    Ok(())
  }
}

/// Gets current branch via `git branch --show-current`
fn get_current_branch() -> Option<String> {
  let output = Command::new("git")
    .args(["branch", "--show-current"])
    .output()
    .ok()?;
  String::from_utf8(output.stdout).ok()
}

fn fetch_all() -> CliResult {
  // -p = prune remote refs (e.g. all the origin/<branch>)
  // -t = fetch tags too
  // --all = from all remotes
  await_child!(
    git!("fetch", "-pt", "--all")?,
    "Failed to fetch from remotes"
  )
}

fn get_term_width() -> usize {
  let (_height, width) = console::Term::stdout().size();
  width as usize
}
