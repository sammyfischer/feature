use anyhow::Result;

use crate::App;
use crate::cli::stash::drop::DropArgs;
use crate::cli::stash::list::ListArgs;
use crate::cli::stash::pop::PopArgs;
use crate::cli::stash::push::PushArgs;
use crate::cli::stash::show::ShowArgs;

mod drop;
mod list;
mod pop;
mod push;
mod show;

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Manage branch stashes", disable_help_subcommand = true)]
pub struct Args {
  /// The branch to create the stash on. Defaults to current branch.
  #[arg(short, long)]
  branch: Option<String>,

  #[command(subcommand)]
  command: StashCommand,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub enum StashCommand {
  Push(PushArgs),
  Pop(PopArgs),
  Drop(DropArgs),
  List(ListArgs),
  Show(ShowArgs),
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    match &self.command {
      StashCommand::Push(args) => args.run(state),
      StashCommand::Pop(args) => args.run(state),
      StashCommand::Drop(args) => args.run(state),
      StashCommand::List(args) => args.run(state),
      StashCommand::Show(args) => args.run(state),
    }
  }
}
