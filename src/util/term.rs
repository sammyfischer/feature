use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::Result;
use console::Term;
use dialoguer::Confirm;

use crate::await_child;

pub fn is_term() -> bool {
  Term::stdout().is_term()
}

pub fn get_term_width() -> usize {
  let (_rows, cols) = console::Term::stdout().size_checked().unwrap_or((64, 80));
  cols as usize
}

/// Configues a yes/no prompt and gets user input
pub fn get_user_confirmation(prompt: &str) -> Result<bool> {
  let result = Confirm::new()
    .default(false)
    .with_prompt(prompt)
    .interact()?;
  Ok(result)
}

/// Takes a string and sends its output to less with the following options:
/// - `-F` to print to stdout directly if the terminal is tall enough
/// - `-R` to print raw control characters
pub fn paginate(s: &str) -> Result<()> {
  let mut less_proc = Command::new("less")
    .arg("-FR")
    .stdin(Stdio::piped())
    .spawn()
    .expect("Failed to start less");

  let stdin = less_proc
    .stdin
    .as_mut()
    .expect("Failed to send output to less");

  stdin
    .write_all(s.as_bytes())
    .expect("Failed to send output to less");

  await_child!(less_proc, "Less")
}
