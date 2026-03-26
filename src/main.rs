use crate::cli::{Cli, CliResult};

mod cli;
mod config;
mod data;

fn main() -> CliResult {
  Cli::new().run()
}
