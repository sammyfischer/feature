use anyhow::Result;

use crate::cli::Cli;

mod cli;
mod config;
mod data;
mod templater;

fn main() -> Result<()> {
  Cli::new().run()
}
