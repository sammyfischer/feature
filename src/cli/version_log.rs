use anyhow::{Context, Result, anyhow};

use crate::core::version::find_current_version;
use crate::{App, git};

const LONG_ABOUT: &str = r#"Displays a git log since last version reachable by a commit. By default, uses
HEAD."#;

#[derive(clap::Args, Clone, Debug)]
#[command(
  visible_alias = "verlog",
  about = "Displays a git log since last version",
  long_about = LONG_ABOUT,
  disable_help_subcommand = true
)]
pub struct VersionLogArgs {
  /// The commit to start from
  #[arg(value_name = "REVISION")]
  rev: Option<String>,
}

impl VersionLogArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo = &state.repo;

    let rev = self.rev.as_deref().unwrap_or("HEAD");
    let commit = repo.revparse_single(rev)?.peel_to_commit()?;

    let (version, _) = find_current_version(repo, &state.config, commit.id())?
      .with_context(|| format!("Failed to find a version reachable from {}", rev))?;

    let status = git!("log", format!("{}..", version.name())).status()?;

    if status.success() {
      Ok(())
    } else {
      Err(anyhow!("Git failed with exit status: {:?}", status.code()))
    }
  }
}
