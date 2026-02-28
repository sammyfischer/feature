use crate::cli::CliResult;
use crate::cli::def::Cli;

mod cli;
mod config;

fn main() -> CliResult {
  let config = config::read().unwrap_or_default();
  let mut cli = Cli::new(config);
  cli.run()
}
