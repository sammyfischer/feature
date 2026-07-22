use std::io;

use anyhow::Result;
use clap::CommandFactory;
use clap_complete::{Shell, generate};

const LONG_ABOUT: &str = r#"Prints a shell completion script for the given shell.

You can redirect this output to the proper location for your shell, e.g.
feature completions bash > ~/bash_completion.d/feature

Or you can source it in your shell config, e.g.
echo 'eval "$(feature completions bash)"' >> ~/.bashrc"#;

#[derive(clap::Args, Clone, Debug)]
#[command(
  about = "Generate shell completion scripts",
  long_about = LONG_ABOUT,
  disable_help_subcommand = true
)]
pub struct CompletionsArgs {
  pub shell: Shell,
}

impl CompletionsArgs {
  pub fn run(&self) -> Result<()> {
    let mut cmd = crate::cli::Args::command();
    let name = cmd.get_name().to_string();

    match self.shell {
      Shell::Bash => print!("{}", include_str!("./completions/bash")),
      _ => generate(self.shell, &mut cmd, name, &mut io::stdout()),
    }

    Ok(())
  }
}
