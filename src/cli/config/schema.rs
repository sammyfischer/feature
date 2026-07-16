use anyhow::Result;
use schemars::schema_for;

use crate::core::project_config::ProjectConfig;

#[derive(clap::Args, Clone, Debug)]
#[command(disable_help_subcommand = true)]
pub struct SchemaArgs {}

impl SchemaArgs {
  pub fn run(&self) -> Result<()> {
    let schema = schema_for!(ProjectConfig);
    let json = serde_json::to_string_pretty(&schema)?;
    println!("{}", json);
    Ok(())
  }
}
