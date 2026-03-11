use crate::cli::CliResult;
use crate::cli::def::Cli;

mod cli;
mod config;

fn main() -> CliResult {
  Cli::new().run()
}
