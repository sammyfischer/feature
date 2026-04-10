//! Helper functions to display formatted strings. Diff-realted display functions can be found in
//! [super::diff]

use console::style;
use git2::{Oid, Signature};

use crate::lossy;

pub fn trim_hash(id: &Oid) -> String {
  id.to_string()[..7].to_string()
}

pub fn display_hash(id: &Oid) -> String {
  style(trim_hash(id)).yellow().to_string()
}

/// Displays the name in cyan, email in dim (gray), and "no one" in red if there is no configured
/// signature. Errors if any error (other than not having a signature) is encountered.
pub fn display_signature(signature: Option<&Signature>) -> String {
  match signature {
    Some(it) => {
      let name = lossy!(it.name_bytes());
      let email = lossy!(it.email_bytes());
      format!("{} {}", style(name).cyan(), style(email).dim())
    }
    None => style("no one").red().to_string(),
  }
}

/// Displays two numbers like `+p -m` where the first part is green and the second part is red.
///
/// The numbers are passed in as a tuple, where the first number is the plus and second is the
/// minus.
///
/// This is used to print ahead/behind and insertions/deletions.
pub fn display_plus_minus(plus: usize, minus: usize) -> String {
  format!(
    "{} {}",
    style(format!("+{}", plus)).green(),
    style(format!("-{}", minus)).red()
  )
}
