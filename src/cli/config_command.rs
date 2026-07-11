//! Config subcommand

use anyhow::Result;
use clap::{Subcommand, ValueHint};
use schemars::schema_for;
use serde::{Deserialize, Serialize};

use crate::core::project_config::{self, ProjectConfig};
use crate::core::term::get_user_confirmation;

/// Serializes the value into a toml string
macro_rules! toml_stringify {
  ($opt:expr) => {
    toml::Value::from($opt).to_string()
  };
}

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Interact with feature config", disable_help_subcommand = true)]
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

  /// Get the value of some config keys. These are the values that feature will
  /// use at runtime
  Get(GetArgs),

  /// Prints an entire schema of the config to stdout
  Schema,
}

#[derive(Clone, Debug, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum WhichConfig {
  /// Local project config file
  Local,
  /// Global project config file
  Global,
}

#[derive(clap::Args, Clone, Debug)]
#[command(disable_help_subcommand = true)]
pub struct GetArgs {
  /// The names of the keys to get
  #[arg(trailing_var_arg = true, value_hint = ValueHint::Other)]
  pub keys: Vec<String>,
}

impl Args {
  pub fn run(&self, config: &ProjectConfig) -> Result<()> {
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
      WhichConfig::Local => {
        // if it already exists, prompt user for confirmation
        if project_config::local::path().exists() {
          let choice = get_user_confirmation(
            "A local config file already exists. Do you want to overwrite it?",
          )?;

          // user selected no
          if !choice {
            return Ok(());
          }
        }
        project_config::local::save_default()
      }

      WhichConfig::Global => {
        // if it already exists, prompt user for confirmation
        if project_config::global::path()?.exists() {
          let choice = get_user_confirmation(
            "A global config file already exists. Do you want to overwrite it?",
          )?;

          // user selected no
          if !choice {
            return Ok(());
          }
        }
        project_config::global::save_default()
      }
    }
  }

  pub fn get(&self, args: &GetArgs, config: &ProjectConfig) -> Result<()> {
    for key in &args.keys {
      let value = match &**key {
        "default_remote" => config.default_remote.clone(),
        "protect" => toml_stringify!(config.protect.clone()),

        "branch.sep" => config.branch.sep.clone(),
        "branch.template" => match config.branch.template {
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
  let schema = schema_for!(ProjectConfig);
  let json = serde_json::to_string_pretty(&schema)?;
  println!("{}", json);
  Ok(())
}
