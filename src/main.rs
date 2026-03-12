use crate::cli::{Cli, CliResult};

mod cli;
mod config;

fn main() -> CliResult {
  Cli::new().run()
}
