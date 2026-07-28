//! Helper functions to display formatted strings for terminal printing

use anyhow::Result;
use console::style;
use git2::{Object, Signature};

use crate::core::string::ToStrLossy;
use crate::core::trim_hash;

pub mod commit;
pub mod diff;
pub mod time;

/// Creates a [StyledObject] with format args
///
/// # Params
/// Format args
///
/// [StyledObject]: console::StyledObject
#[macro_export]
macro_rules! style {
  ($($arg:tt)*) => {
    console::style(&format!($($arg)*))
  };
}

/// Creates styled text wrapped in dim white (gray) square brackets
///
/// # Params
/// Format args
#[macro_export]
macro_rules! dim_brackets {
  ($($arg:tt)*) => {
    format!("{}{}{}", console::style("[").dim(), $crate::style!($($arg)*), console::style("]").dim())
  };
}

/// Creates an if expression that uses the given text if `cond` is true, else
/// the empty string.
///
/// # First form
/// `nerdfont!(cond: expr, text: string literal)`
/// - `cond` - the condition of the if expression
/// - `text` - the value to return if `cond` is true
///
/// # Second form
/// `nerdfont!(cond: expr, yes: string literal, no: string literal)`
/// - `cond` - the condition of the if expression
/// - `yes` - the value to return if `cond` is true
/// - `no` - the value to return if `cond` is false
#[macro_export]
macro_rules! if_nerdfont {
  ($cond:expr, $text:literal) => {
    if $cond { $text } else { "" }
  };

  ($cond:expr, $yes:literal, $no:literal) => {
    if $cond { $yes } else { $no }
  };
}

/// Displays a trimmed hash in yellow
pub fn display_hash(obj: &Object) -> Result<String> {
  Ok(style(trim_hash(obj)?).yellow().to_string())
}

/// Displays the name in cyan, email in dim (gray), and "No user info" in red
/// if there is no configured signature.
pub fn display_signature(signature: Option<&Signature>) -> String {
  match signature {
    Some(it) => {
      let name = it.name_bytes().to_str_lossy();
      let email = it.email_bytes().to_str_lossy();
      format!("{} {}", style(name).cyan(), style(email).dim())
    }
    None => style("No user info").red().to_string(),
  }
}

/// Displays two numbers like `+p -m` where the first part is green and the
/// second part is red.
///
/// This is used to print ahead/behind and insertions/deletions.
pub fn display_plus_minus(plus: usize, minus: usize) -> String {
  format!(
    "{} {}",
    style!("+{}", plus).green(),
    style!("-{}", minus).red()
  )
}
