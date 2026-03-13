//! Useful functions, macros, and core command implementation

use std::io::{IsTerminal, Write};
use std::process::{Command, Stdio};

use clap::{Parser, Subcommand};
use unicode_width::UnicodeWidthChar;

use crate::cli::error::CliError;
use crate::config::Config;

mod commit;
mod config;
pub mod error;
mod start;

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
  Update {
    /// The name of the base branch to use. Defaults to the repo's main base
    base: Option<String>,
  },

  /// Push current branch to remote
  Push,

  // ==== REPO / MULTI BRANCH MANAGEMENT ====
  /// Syncs all base (protected) branches with remotes. Only fast-forwards branches, refuses to
  /// rebase/merge
  Sync,

  /// Clean up merged branches. A branch is merged if all its commits are found on default_base
  Prune {
    #[arg(long = "dry-run")]
    dry_run: bool,
  },

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
      Action::Update { base } => self.update(base),
      Action::Push => self.push(),
      Action::Sync => self.sync(),
      Action::Prune { dry_run } => self.prune(*dry_run),
      Action::List => self.list(),
      Action::Log => self.log(),
      Action::Graph => self.graph(),
      Action::Config { args } => args.run(),
    }
  }

  /// Update current branch with base branch (using rebase)
  fn update(&self, base: &Option<String>) -> CliResult {
    let branch = match base {
      Some(it) => it,
      None => &self.config.default_base,
    };

    await_child!(git!("rebase", branch).spawn()?, "Failed to call git")
  }

  fn push(&self) -> CliResult {
    let branch = get_current_branch()?;

    if self.config.protected_branches.contains(&branch) {
      eprintln!("This is a protected branch, refusing to push");
      return Ok(());
    }

    // -u = set upstream, doesn't hurt to do this every time
    // --force-with-lease = protects against overwriting others' work, but allows pushing after
    // rebasing with main (since that changes commit history)
    await_child!(
      git!(
        "push",
        "-u",
        "--force-with-lease",
        &self.config.default_remote,
        branch
      )
      .spawn()?,
      "Failed to push"
    )
  }

  fn sync(&self) -> CliResult {
    fetch_all()?;

    if has_local_changes()? {
      return Err(CliError::Generic(
        "You have uncommitted changes! Please commit or stash them before syncing".into(),
      ));
    }

    // save current branch to switch back to at the end
    let start_branch = get_current_branch()?;

    // TODO: should protected branches be considered base branches?
    let base_branches = &self.config.protected_branches;

    // whether the script switched to a diffferent branch
    let mut has_switched = false;

    // error messages to print at the end
    let mut failures: Vec<String> = Vec::new();

    for branch in base_branches {
      // switch to branch
      let Ok(mut child) = git!("switch", branch).spawn() else {
        failures.push(format!("Failed to switch to branch: {}", branch));
        continue;
      };
      let Ok(_) = await_child!(child, format!("Failed to switch to branch: {}", branch)) else {
        failures.push(format!("Failed to switch to branch: {}", branch));
        continue;
      };

      has_switched = true;

      if let Ok(yes) = can_fast_forward(branch) {
        if !yes {
          failures.push(format!(
            "Cannot fast forward branch: {}. You might want to pull manually",
            branch
          ));
        }
      } else {
        failures.push(format!("Failed to check if {} is fast-forwardable", branch));
        continue;
      }

      // pull changes (fast-forward only)
      let Ok(mut child) = git!("pull", "--ff-only").spawn() else {
        failures.push(format!("Failed to pull changes into branch: {}", branch));
        continue;
      };
      let Ok(_) = await_child!(
        child,
        format!("Failed to pull changes into branch: {}", branch)
      ) else {
        failures.push(format!("Failed to pull changes into branch: {}", branch));
        continue;
      };
    }

    if has_switched {
      // switch back
      if let Ok(mut child) = git!("switch", &start_branch).spawn()
        && await_child!(child, "Failed to switch back to starting branch").is_err()
      {
        failures.push(format!(
          "Failed to switch back to starting branch: {}",
          &start_branch
        ));
      };
    }

    if !failures.is_empty() {
      eprintln!("{}", failures.join("\n"));
    }

    Ok(())
  }

  fn prune(&self, dry_run: bool) -> CliResult {
    fetch_all()?;

    // get list of all branches
    let branches = get_all_branches()?;

    if dry_run {
      println!("Deletion candidates:")
    }

    for branch in branches {
      // skip protected branches
      if self.config.protected_branches.contains(&branch) {
        continue;
      }

      // skip current branch
      let current_branch = get_current_branch();
      if current_branch.is_ok_and(|it| it == branch) {
        continue;
      }

      // detect if branch is merged (i.e. has no commits that aren't on main)
      if is_merged(&branch, &self.config.default_base).is_ok_and(|yes| yes) {
        // in dry-run mode, print the branch name but don't delete
        if dry_run {
          println!("{}", &branch);
          continue;
        }

        // delete 1 by 1 (use -D to force delete, we've assured all commits are on main)
        if let Ok(mut child) = git!("branch", "-D", &branch).spawn() {
          if await_child!(child, format!("Failed to delete branch {}", &branch)).is_err() {
            eprintln!("Failed to delete branch {}", &branch);
          }
        } else {
          eprintln!("Failed to delete branch {}", &branch);
        };
      }
    }

    Ok(())
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
fn get_tracking(branch: &str) -> CliResult<String> {
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
  let remote = get_tracking(branch)?;

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
