use anyhow::Result;

use crate::cli::config::WhichConfig;
use crate::cli::term::get_user_confirmation;
use crate::core::project_config::{global, local};

#[derive(clap::Args, Clone, Debug)]
#[command(disable_help_subcommand = true)]
pub struct CreateArgs {}

impl CreateArgs {
  pub fn run(&self, which: &WhichConfig) -> Result<()> {
    match which {
      WhichConfig::Local => {
        // if it already exists, prompt user for confirmation
        if local::path().exists() {
          let choice = get_user_confirmation(
            "A local config file already exists. Do you want to overwrite it?",
          )?;

          // user selected no
          if !choice {
            return Ok(());
          }
        }
        local::save_default()
      }

      WhichConfig::Global => {
        // if it already exists, prompt user for confirmation
        if global::path()?.exists() {
          let choice = get_user_confirmation(
            "A global config file already exists. Do you want to overwrite it?",
          )?;

          // user selected no
          if !choice {
            return Ok(());
          }
        }
        global::save_default()
      }
    }
  }
}
