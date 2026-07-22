//! Config subcommand

use anyhow::Result;
use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::cli::config::create::CreateArgs;
use crate::cli::config::get::GetArgs;
use crate::cli::config::schema::SchemaArgs;
use crate::core::project_config::ProjectConfig;

mod create;
mod get;
mod schema;

/// Serializes the value into a toml string
#[macro_export]
macro_rules! toml_stringify {
  ($opt:expr) => {
    toml::Value::from($opt).to_string()
  };
}

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Interact with feature config", disable_help_subcommand = true)]
pub struct ConfigArgs {
  /// Which config file to use
  #[arg(long, default_value = "local", conflicts_with = "global")]
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
  Create(CreateArgs),

  /// Get the value of some config keys. These are the values that feature will
  /// use at runtime
  Get(GetArgs),

  /// Prints an entire schema of the config to stdout
  Schema(SchemaArgs),
}

#[derive(Clone, Debug, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum WhichConfig {
  /// Local project config file
  Local,
  /// Global project config file
  Global,
}

impl ConfigArgs {
  pub fn run(&self, config: &ProjectConfig) -> Result<()> {
    let which = if self.global {
      &WhichConfig::Global
    } else {
      &self.which
    };

    match &self.command {
      ConfigCommand::Create(args) => args.run(which),
      ConfigCommand::Get(args) => args.run(config),
      ConfigCommand::Schema(args) => args.run(),
    }
  }
}
