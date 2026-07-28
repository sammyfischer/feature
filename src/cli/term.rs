//! Helper functions pertaining to the terminal

use std::borrow::Cow;
use std::io::{ErrorKind, Write};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow};
use console::{Term, truncate_str};
use dialoguer::Confirm;

/// Tick strings to use for loading spinners
pub const TICK_STRINGS: [&str; 11] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "✓"];

/// Characters to use for progress bars
pub const PROGRESS_CHARS: &str = "=> ";

pub fn is_term() -> bool {
  Term::stdout().is_term()
}

pub fn get_term_width() -> usize {
  let (_rows, cols) = console::Term::stdout().size_checked().unwrap_or((64, 80));
  cols as usize
}

/// Truncates a string to term width if stdout is a terminal
pub fn trunc_term_width<'s>(s: &'s str, tail: &str) -> Cow<'s, str> {
  if is_term() {
    truncate_str(s, get_term_width(), tail)
  } else {
    Cow::Borrowed(s)
  }
}

/// Configues a yes/no prompt and gets user input. Prompts with "no" as the
/// default.
pub fn get_user_confirmation(prompt: &str) -> Result<bool> {
  let result = Confirm::new()
    .default(false)
    .with_prompt(prompt)
    .interact()?;
  Ok(result)
}

/// Sends bytes to less with the following options:
/// - `-F` to print to stdout directly if the terminal is tall enough
/// - `-R` to print raw control characters
/// - `-S` to turn off line-wrapping
pub fn paginate(buf: &[u8]) -> Result<()> {
  let mut cmd = Command::new("less")
    .arg("-FRS")
    .stdin(Stdio::piped())
    .spawn()
    .context("Failed to start pager")?;

  match cmd
    .stdin
    .as_mut()
    .ok_or(anyhow!("Failed to open pipe to pager"))?
    .write_all(buf)
  {
    Ok(_) => {}
    // broken pipe will happen if the user exits less but there's still more output
    Err(e) if e.kind() == ErrorKind::BrokenPipe => {}
    Err(e) => return Err(anyhow!(e).context("Failed to write to pager")),
  };

  cmd.wait()?;
  Ok(())
}
