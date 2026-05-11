//! Config subcommand

use anyhow::Result;
use clap::{Subcommand, ValueHint};
use schemars::schema_for;
use serde::{Deserialize, Serialize};

use crate::config::{self, Config};
use crate::util::term::get_user_confirmation;

/// Serializes the value into a toml string
macro_rules! toml_stringify {
  ($opt:expr) => {
    toml::Value::from($opt).to_string()
  };
}

#[derive(clap::Args, Clone, Debug)]
#[command(
  about = "Interact with feature config",
  disable_help_flag = true,
  disable_help_subcommand = true
)]
pub struct Args {
  /// Which config file to use
  #[arg(long, default_value = "project", conflicts_with = "global")]
  pub which: WhichConfig,

  /// Shorthand for --which=global
  #[arg(short, long, conflicts_with = "which")]
  pub global: bool,

  #[command(subcommand)]
  pub command: ConfigCommand,
}

#[derive(Clone, Debug, Subcommand)]
pub enum ConfigCommand {
  /// Creates a config file with default values
  Create,

  /// Get the value of some config keys. These are the values that feature will use at runtime
  Get(GetArgs),

  /// Prints an entire schema of the config to stdout
  Schema,
}

#[derive(Clone, Debug, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum WhichConfig {
  /// Project (local) config file
  Project,
  /// Project (local) config file
  Local,
  /// Global (user) config file
  User,
  /// Global (user) config file
  Global,
}

#[derive(clap::Args, Clone, Debug)]
#[command(disable_help_flag = true, disable_help_subcommand = true)]
pub struct GetArgs {
  /// The names of the keys to get
  #[arg(trailing_var_arg = true, value_hint = ValueHint::Other)]
  pub keys: Vec<String>,
}

impl Args {
  pub fn run(&self, config: &Config) -> Result<()> {
    let which = if self.global {
      &WhichConfig::Global
    } else {
      &self.which
    };

    match &self.command {
      ConfigCommand::Create => self.create(which),
      ConfigCommand::Get(args) => self.get(args, config),
      ConfigCommand::Schema => generate_schema(),
    }
  }

  pub fn create(&self, which: &WhichConfig) -> Result<()> {
    match which {
      WhichConfig::Project | WhichConfig::Local => {
        // if it already exists, prompt user for confirmation
        if config::project::path().exists() {
          let choice = get_user_confirmation(
            "A local config file already exists. Do you want to overwrite it?",
          )?;

          // user selected no
          if !choice {
            return Ok(());
          }
        }
        config::project::save_default()
      }

      WhichConfig::User | WhichConfig::Global => {
        // if it already exists, prompt user for confirmation
        if config::user::path()?.exists() {
          let choice = get_user_confirmation(
            "A global config file already exists. Do you want to overwrite it?",
          )?;

          // user selected no
          if !choice {
            return Ok(());
          }
        }
        config::user::save_default()
      }
    }
  }

  pub fn get(&self, args: &GetArgs, config: &Config) -> Result<()> {
    for key in &args.keys {
      let value = match &**key {
        "default_remote" => config.default_remote.clone(),
        "protect" => toml_stringify!(config.protect.clone()),

        "end.prune" => toml_stringify!(config.end.remote),

        "sync.prune" => toml_stringify!(config.sync.prune),

        "status.show_untracked" => toml_stringify!(config.status.show_untracked),

        "list.hash" => toml_stringify!(config.list.hash),
        "list.upstream" => toml_stringify!(config.list.upstream),
        "list.base" => toml_stringify!(config.list.base),

        "show.message" => config.show.message.to_string(),
        "show.summary" => toml_stringify!(config.show.summary),
        "show.patch" => toml_stringify!(config.show.patch),
        "show.paging" => config.show.paging.to_string(),

        "format.branch_sep" => config.format.branch_sep.clone(),
        "format.branch" => match config.format.branch {
          Some(ref it) => it.clone(),
          None => "None".to_string(),
        },

        key => {
          eprintln!("Unrecognized key: {}", key);
          continue;
        }
      };

      println!("{}: {}", key, value);
    }

    Ok(())
  }
}

fn generate_schema() -> Result<()> {
  let schema = schema_for!(Config);
  let json = serde_json::to_string_pretty(&schema)?;
  println!("{}", json);
  Ok(())
}
