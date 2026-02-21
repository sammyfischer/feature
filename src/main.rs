use crate::cli::{def::Cli, CliResult};
use crate::config::{Config, read_config};

mod cli;
mod config;

fn main() -> CliResult {
  let config = match read_config() {
    Ok(config) => config,
    Err(_) => Config::default(),
  };

  let mut cli = Cli::new(config);
  cli.run()
}
