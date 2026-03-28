use anyhow::Result;

use crate::cli::Cli;

mod cli;
mod config;
mod data;

fn main() -> Result<()> {
  Cli::new().run()
}
