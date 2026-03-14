use crate::cli::{Cli, CliResult};

mod cli;
mod config;
mod database;

fn main() -> CliResult {
  Cli::new().run()
}
