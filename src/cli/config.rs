//! Config subcommand

use clap::Subcommand;

use crate::cli::CliResult;
use crate::config;

/// Implements get logic for each config key
macro_rules! config_get {
  ($doc:expr, $args:expr; $($key:ident),+ $(,)?) => {
    $(
      if $args.$key {
        match $doc.get(stringify!($key)) {
          Some(it) => println!("{}: {}", stringify!($key), it.to_string().trim()),
          None => println!("{} is unset", stringify!($key)),
        }
      }
    )+
  };
}

/// Implements set logic for each config key
macro_rules! config_set {
  ($doc:expr, $args:expr; $($key:ident),+ $(,)?) => {
    $(
      if let Some(it) = & $args.$key {
        $doc[stringify!($key)] = toml_edit::value(it);
      }
    )+
  };
}

/// Implements unset logic for each config key
macro_rules! config_unset {
  ($doc:expr, $args:expr; $($key:ident),+ $(,)?) => {
    $(
      if $args.$key {
        let old_val = $doc.remove_entry(stringify!($key));
        if let Some((_, item)) = old_val {
          println!(
            "Unset {} (was {})",
            stringify!($key),
            item.to_string().trim()
          );
        }
      }
    )+
  };
}

#[derive(Clone, Debug, Subcommand)]
pub enum Args {
  Get(GetArgs),
  Set(SetArgs),
  #[command(visible_aliases = ["del", "delete"])]
  Unset(UnsetArgs),
}

#[derive(clap::Args, Clone, Debug)]
pub struct GetArgs {
  #[arg(long, alias = "default_base")]
  pub default_base: bool,

  #[arg(long, alias = "default_remote")]
  pub default_remote: bool,

  #[arg(long, alias = "branch_sep")]
  pub branch_sep: bool,
}

#[derive(clap::Args, Clone, Debug)]
pub struct SetArgs {
  #[arg(long, alias = "default_base")]
  pub default_base: Option<String>,

  #[arg(long, alias = "default_remote")]
  pub default_remote: Option<String>,

  #[arg(long, alias = "branch_sep")]
  pub branch_sep: Option<String>,
}

#[derive(clap::Args, Clone, Debug)]
pub struct UnsetArgs {
  #[arg(long, alias = "default_base")]
  pub default_base: bool,

  #[arg(long, alias = "default_remote")]
  pub default_remote: bool,

  #[arg(long, alias = "branch_sep")]
  pub branch_sep: bool,
}

impl Args {
  pub fn run(&self) -> CliResult {
    match self {
      Args::Get(args) => self.get(args),
      Args::Set(args) => self.set(args),
      Args::Unset(args) => self.unset(args),
    }
  }

  pub fn get(&self, args: &GetArgs) -> CliResult {
    let doc = config::read_doc()?;
    config_get!(doc, args; default_base, default_remote, branch_sep);
    Ok(())
  }

  pub fn set(&self, args: &SetArgs) -> CliResult {
    let mut doc = config::read_doc()?;
    config_set!(doc, args; default_base, default_remote, branch_sep);
    config::write(&doc)?;
    Ok(())
  }

  pub fn unset(&self, args: &UnsetArgs) -> CliResult {
    let mut doc = config::read_doc()?;
    config_unset!(doc, args; default_base, default_remote, branch_sep);
    config::write(&doc)?;
    Ok(())
  }
}
