use anyhow::Result;
use clap::ValueHint;
use figment::Figment;
use figment::providers::Serialized;

use crate::core::project_config::ProjectConfig;

#[derive(clap::Args, Clone, Debug)]
#[command(disable_help_subcommand = true)]
pub struct GetArgs {
  /// The names of the keys to get
  #[arg(trailing_var_arg = true, value_hint = ValueHint::Other)]
  pub keys: Vec<String>,
}

impl GetArgs {
  pub fn run(&self, config: &ProjectConfig) -> Result<()> {
    let figment = Figment::new().merge(Serialized::defaults(config));

    for key in &self.keys {
      let value = figment.extract_inner::<String>(key)?;
      println!("{} = {}", key, value);
    }

    Ok(())
  }
}
