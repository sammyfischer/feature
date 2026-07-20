use anyhow::Result;
use console::style;

use crate::App;
use crate::cli::wip::drop::DropArgs;
use crate::cli::wip::list::ListArgs;
use crate::cli::wip::pop::PopArgs;
use crate::cli::wip::push::PushArgs;
use crate::cli::wip::show::ShowArgs;
use crate::core::wip::Wip;

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

/// Displays the wip's spec with colors
pub fn display_wip(wip: &Wip) -> String {
  format!(
    "{}{}{}",
    style(wip.branch()).cyan(),
    style(":").dim(),
    style(wip.index()).cyan()
  )
}

/// Displays a wipspec with colors
pub fn display_wipspec(branch: &str, index: usize) -> String {
  format!(
    "{}{}{}",
    style(branch).cyan(),
    style(":").dim(),
    style(index).cyan()
  )
}
