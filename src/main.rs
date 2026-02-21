use crate::cli::{def::Cli, CliResult};
use crate::config::read_config;

mod cli;
mod config;

fn main() -> CliResult {
  let config = read_config().unwrap_or_default();
  let mut cli = Cli::new(config);
  cli.run()
}
