use anyhow::Result;
use clap::ValueHint;

use crate::core::project_config::ProjectConfig;
use crate::toml_stringify;

#[derive(clap::Args, Clone, Debug)]
#[command(disable_help_subcommand = true)]
pub struct GetArgs {
  /// The names of the keys to get
  #[arg(trailing_var_arg = true, value_hint = ValueHint::Other)]
  pub keys: Vec<String>,
}

impl GetArgs {
  pub fn run(&self, config: &ProjectConfig) -> Result<()> {
    for key in &self.keys {
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
