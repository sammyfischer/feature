use anyhow::Result;

use crate::App;
use crate::cli::wip::drop::DropArgs;
use crate::cli::wip::list::ListArgs;
use crate::cli::wip::pop::PopArgs;
use crate::cli::wip::push::PushArgs;
use crate::cli::wip::show::ShowArgs;

mod drop;
mod list;
mod pop;
mod push;
mod show;

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Manage branch wips", disable_help_subcommand = true)]
pub struct Args {
  #[command(subcommand)]
  command: WipCommand,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub enum WipCommand {
  Push(PushArgs),
  Pop(PopArgs),
  Drop(DropArgs),
  List(ListArgs),
  Show(ShowArgs),
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    match &self.command {
      WipCommand::Push(args) => args.run(state),
      WipCommand::Pop(args) => args.run(state),
      WipCommand::Drop(args) => args.run(state),
      WipCommand::List(args) => args.run(state),
      WipCommand::Show(args) => args.run(state),
    }
  }
}
