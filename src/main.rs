#![feature(trim_prefix_suffix)]

use anyhow::Result;

use crate::cli::Cli;

mod cli;
mod config;
mod data;
mod templater;
mod util;

fn main() -> Result<()> {
  Cli::new().run()
}
