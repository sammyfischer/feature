use anyhow::Result;
use clap::Subcommand;

use crate::App;
use crate::cli::project::add::AddArgs;
use crate::cli::project::each::EachArgs;
use crate::cli::project::list::ListArgs;
use crate::cli::project::remove::RemoveArgs;

mod add;
mod each;
mod list;
mod remove;

const LONG_ABOUT: &str = r"Interact with feature projects.

Feature projects are a way of including other git repos as subprojects of this
repo. This allows you to create a monorepo out of a project that spans multiple
repos.";

#[derive(clap::Args, Debug)]
#[command(
  about = "Interact with projects",
  visible_alias = "proj",
  long_about = LONG_ABOUT,
  disable_help_subcommand = true
)]
pub struct Args {
  #[command(subcommand)]
  command: ProjectCommand,
}

#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
  Add(AddArgs),
  Remove(RemoveArgs),
  List(ListArgs),
  Each(EachArgs),
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    match &self.command {
      ProjectCommand::Add(args) => args.run(state),
      ProjectCommand::Remove(args) => args.run(state),
      ProjectCommand::List(args) => args.run(state),
      ProjectCommand::Each(args) => args.run(state),
    }
  }
}
