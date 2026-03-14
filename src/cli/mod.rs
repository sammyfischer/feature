//! Defines the main cli structure, most simple commands, and several helper functions and macros.

use std::io::{IsTerminal, Write};
use std::process::{Command, Stdio};

use clap::{Parser, Subcommand};
use unicode_width::UnicodeWidthChar;

use crate::cli::error::CliError;
use crate::config::Config;
use crate::database;

mod commit;
mod config;
pub mod error;
mod prune;
mod push;
mod start;
mod sync;
mod update;

/// Waits on the child process, returns result
#[macro_export]
macro_rules! await_child {
  ($child:expr, $msg:expr) => {
    if $child.wait().is_ok_and(|status| status.success()) {
      Ok(())
    } else {
      Err($crate::cli::error::CliError::SubprocessFailed(
        $msg.to_string(),
      ))
    }
  };
}

/// Spawns a git command, passing this macros args as command line args
#[macro_export]
macro_rules! git {
  ($($arg:expr),* $(,)?) => {
    {
      let mut cmd = std::process::Command::new("git");
      $(
        cmd.arg($arg);
      )*
      cmd
    }
  };
}

pub type CliResult<T = ()> = Result<T, CliError>;

#[derive(Debug, Parser)]
pub struct Args {
  #[command(subcommand)]
  pub action: Action,
}

#[derive(Debug, Subcommand)]
pub enum Action {
  // ==== FEATURE BRANCH WORKFLOW / SINGLE BRANCH ACTIONS ====
  /// Start a new feature branch
  Start(start::Args),

  /// Commit using remaining args as commit message
  Commit(commit::Args),

  /// Update the current branch against its base branch
  Update(update::Args),

  /// Push current branch to remote
  Push(push::Args),

  // ==== REPO / MULTI BRANCH MANAGEMENT ====
  /// Syncs all base (protected) branches with remotes. Only fast-forwards branches, refuses to
  /// rebase/merge
  Sync,

  /// Clean up merged branches. A branch is merged if all its commits are found on default_base
  Prune(prune::Args),

  // ==== DISPLAY / INFO ====
  /// List branches
  #[command(visible_alias = "ls")]
  List,

  /// View git log with entire commit subject line, followed by author name and relative date
  Log,

  /// View git graph with author name and relative date in noticable colors. Truncates each line to
  /// the terminal width
  Graph,

  // ==== META / FEATURE COMMANDS ====
  /// Interact with feature config
  Config {
    #[command(subcommand)]
    args: config::Args,
  },

  /// Set base branch in the database
  Base {
    /// The base branch
    base: String,

    /// The branch whose base will be set. Defaults to current branch
    branch: Option<String>,
  },
}

pub struct Cli {
  pub(crate) config: Config,
  pub(crate) args: Args,
}

impl Cli {
  pub fn new() -> Self {
    let config = crate::config::load().unwrap_or_default();
    let args = Args::parse();
    Self { config, args }
  }

  pub fn run(&mut self) -> CliResult {
    match &self.args.action {
      Action::Start(args) => args.run(self),
      Action::Commit(args) => args.run(),
      Action::Update(args) => args.run(),
      Action::Push(args) => args.run(self),
      Action::Sync => sync::sync(self),
      Action::Prune(args) => args.run(self),
      Action::List => self.list(),
      Action::Log => self.log(),
      Action::Graph => self.graph(),
      Action::Config { args } => args.run(),
      Action::Base { base, branch } => self.base(base, branch),
    }
  }

  fn list(&self) -> CliResult {
    await_child!(git!("branch", "-vv").spawn()?, "Failed to call git")
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
      )
      .spawn()?,
      "Failed to call git"
    )
  }

  fn graph(&self) -> CliResult {
    // like log, but author name and date are first and colored
    let output = git!(
      "log",
      "--graph",
      "--all",
      "--color=always",
      "--pretty=format:%C(auto)%h%d %C(green)%an %C(blue)%ar %C(reset)%s",
    )
    .output()?;

    let string_output = String::from_utf8(output.stdout)?;

    // if stdout is not a terminal, just print and return
    if !std::io::stdout().is_terminal() {
      println!("{}", string_output);
      return Ok(());
    }

    // if stdout is a terminal, truncate lines

    let lines = string_output.lines();
    let mut out_lines: Vec<String> = Vec::new();

    let term_width = get_term_width();

    // truncate each line to term width
    for line in lines {
      let mut acc_width = 0usize;
      let mut line_buf = String::new();
      let mut escape_sequence = false;

      // push characters until term width is exceeded
      for c in line.chars() {
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

        // if width returns None, assume 0
        let char_width = c.width().unwrap_or(0);

        if acc_width + char_width > term_width {
          break;
        }

        acc_width += char_width;
        line_buf.push(c);
      }

      // push line to output
      out_lines.push(line_buf);
    }

    let truncated = out_lines.join("\n");

    // forward output to less
    // -F = just print output to stdout if it fits the terminal height
    // -R = render control chars as-is
    let mut less_proc = Command::new("less")
      .arg("-FR")
      .stdin(Stdio::piped())
      .spawn()?;

    let stdin = less_proc.stdin.as_mut().ok_or(CliError::SubprocessFailed(
      "Failed to pipe output to pager".into(),
    ))?;

    stdin.write_all(truncated.as_bytes())?;
    await_child!(less_proc, "Failed to open pager")
  }

  fn base(&self, base: &str, branch: &Option<String>) -> CliResult {
    let branch = match branch {
      Some(it) => it,
      None => &get_current_branch()?,
    };

    let mut db = database::load()?;
    db.insert(branch.to_string(), base.to_string());
    database::save(db)
  }
}

/// Gets current branch via `git branch --show-current`
fn get_current_branch() -> CliResult<String> {
  let output = git!("branch", "--show-current").output()?;
  let output = String::from_utf8(output.stdout)?;
  Ok(output.trim().to_string())
}

/// Returns a list of all local branches, or None if there was an error getting output
fn get_all_branches() -> CliResult<Vec<String>> {
  let output = git!("branch", "--format=%(refname:short)").output()?;
  let output = String::from_utf8(output.stdout)?;
  Ok(output.lines().map(|branch| branch.to_string()).collect())
}

/// Gets the branch's remote tracking branch
fn get_tracking_branch(branch: &str) -> CliResult<String> {
  // --list <pattern> filters the entire list for the given pattern, we can use the current branch
  // name to only find its remote tracking branch
  let output = git!(
    "branch",
    "--format=%(refname:short) %(upstream:short)",
    "--list",
    branch
  )
  .output()?;
  let output = String::from_utf8(output.stdout)?;
  let mut lines = output.lines();

  if let Some(line) = lines.next() {
    let mut words = line.split(" ");
    let _ = words.next();
    let remote = words.next();

    if let Some(it) = remote {
      return Ok(it.to_string());
    }
  }

  Err(CliError::Generic(format!(
    "Couldn't find tracked branch for {}",
    branch
  )))
}

/// Whether branch is merged into base
fn is_merged(branch: &str, base: &str) -> CliResult<bool> {
  let output = git!("log", branch, "--not", base, "--oneline").output()?;
  let output = String::from_utf8(output.stdout)?;
  Ok(output.trim().is_empty())
}

/// Whether there are any uncommitted changes
fn has_local_changes() -> CliResult<bool> {
  // check unstaged changes
  let output = git!("diff")
    .output()
    .map_err(|_| CliError::Generic("Failed to check for uncommitted changes".into()))?;

  let output = String::from_utf8(output.stdout)
    .map_err(|_| CliError::Generic("Failed to check for uncommitted changes".into()))?;

  // if diff is non-empty, immediately return true
  if !output.trim().is_empty() {
    return Ok(true);
  }

  // check staged changes
  let output = git!("diff", "--cached")
    .output()
    .map_err(|_| CliError::Generic("Failed to check for uncommitted changes".into()))?;

  let output = String::from_utf8(output.stdout)
    .map_err(|_| CliError::Generic("Failed to check for uncommitted changes".into()))?;

  Ok(!output.trim().is_empty())
}

/// Whether the branch can be fast-forwarded to its remote counterpart
fn can_fast_forward(branch: &str) -> CliResult<bool> {
  let remote = get_tracking_branch(branch)?;

  let output = git!("rev-parse", branch).output()?;
  let output = String::from_utf8(output.stdout)?;
  let local_sha = output.trim();

  let output = git!("rev-parse", &remote).output()?;
  let output = String::from_utf8(output.stdout)?;
  let remote_sha = output.trim();

  // same sha, branches are up to date
  if local_sha == remote_sha {
    return Ok(true);
  }

  let merge_base = git!("merge-base", "--is-ancestor", branch, remote).status()?;
  Ok(merge_base.success())
}

fn fetch_all() -> CliResult {
  // -p = prune remote refs (e.g. all the origin/<branch>)
  // -t = fetch tags too
  // --all = from all remotes
  await_child!(
    git!("fetch", "-pt", "--all").spawn()?,
    "Failed to fetch from remotes"
  )
}

fn get_term_width() -> usize {
  let (_rows, cols) = console::Term::stdout().size();
  cols as usize
}
