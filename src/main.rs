use crate::cli::{Cli, CliResult};
use crate::config::{Config, read_config};

mod cli;
mod config;

fn main() -> CliResult {
  let config = match read_config() {
    Ok(config) => config,
    Err(e) => {
      // most common error is file not existing, we don't need to print an error message in that
      // case
      eprintln!("{}", e);
      Config::default()
    }
  };

  let mut cli = Cli::new(config);
  cli.run()
}
