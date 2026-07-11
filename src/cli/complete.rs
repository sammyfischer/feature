use anyhow::{Context, Result};
use git2::{BranchType, Repository};

use crate::App;
use crate::core::project_config::ProjectConfig;
use crate::core::string::ToStrLossyOwned;

/// Dynamic shell completions
#[derive(clap::Args, Clone, Debug)]
#[command(hide = true, disable_help_flag = true, disable_help_subcommand = true)]
pub struct Args {
  /// The type of value to complete
  #[arg(short = 't', long = "type")]
  pub comp_type: CompletionType,

  /// The value being completed
  pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum CompletionType {
  Branch,
  Remote,
  Rev,
  Project,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let reply = match self.comp_type {
      CompletionType::Branch => self.find_matching_branches(&state.repo)?,
      CompletionType::Remote => self.find_matching_remotes(&state.repo)?,
      CompletionType::Rev => self.find_matching_revs(&state.repo)?,
      CompletionType::Project => self.find_matching_projects(&state.config)?,
    };
    print_matches(reply);
    Ok(())
  }

  /// Find local branches that match a given prefix
  fn find_matching_branches(&self, repo: &Repository) -> Result<Vec<String>> {
    let prefix = &self.value;
    let mut names = Vec::new();
    let branches = repo.branches(Some(BranchType::Local))?;

    for (branch, _) in branches.flatten() {
      let name = branch.name_bytes()?;
      if name.starts_with(prefix.as_bytes()) {
        names.push(name.to_str_lossy_owned());
      }
    }

    Ok(names)
  }

  /// Find remote names that match a given prefix
  fn find_matching_remotes(&self, repo: &Repository) -> Result<Vec<String>> {
    let prefix = &self.value;
    let mut names = Vec::new();
    let remotes = repo.remotes()?;

    for remote in remotes.iter().flatten() {
      let remote = remote.context("Remote names must be valid utf-8")?;
      if remote.starts_with(prefix) {
        names.push(remote.to_string());
      }
    }

    Ok(names)
  }

  /// Find reasonable revspecs that match the given prefix. This searches local
  /// branches, remote branches, tags, and finally special rev names (e.g.
  /// HEAD).
  fn find_matching_revs(&self, repo: &Repository) -> Result<Vec<String>> {
    let prefix = &self.value;
    let mut names = Vec::with_capacity(100);

    // local branches first
    for (local, _) in repo.branches(Some(BranchType::Local))?.flatten() {
      names.push(local.name_bytes()?.to_str_lossy_owned());
    }

    // then remote branches
    for (remote, _) in repo.branches(Some(BranchType::Remote))?.flatten() {
      names.push(remote.name_bytes()?.to_str_lossy_owned());
    }

    // then tags
    for tag in repo.tag_names(None)?.iter().flatten() {
      let tag = tag.context("Tag names must be valid utf-8")?;
      names.push(tag.to_string());
    }

    let special_revs = [
      "HEAD",
      "ORIG_HEAD",
      "MERGE_HEAD",
      "FETCH_HEAD",
      "CHERRY_PICK_HEAD",
      "REVERT_HEAD",
    ];
    for special in special_revs {
      // filter special revspecs that currently exist
      if repo.revparse_single(special).is_ok() {
        names.push(special.to_string());
      }
    }

    // filter everything at the end
    Ok(
      names
        .into_iter()
        .filter(|name| name.starts_with(prefix))
        .collect(),
    )
  }

  fn find_matching_projects(&self, config: &ProjectConfig) -> Result<Vec<String>> {
    let prefix = &self.value;
    let mut names = Vec::new();

    for (name, _) in &config.projects {
      if name.starts_with(prefix) {
        names.push(name.to_owned());
      }
    }

    Ok(names)
  }
}

fn print_matches(matches: Vec<String>) {
  for (i, name) in matches.iter().enumerate() {
    if i == 0 {
      print!("{}", name);
    } else {
      print!(" {}", name);
    }
  }
}
