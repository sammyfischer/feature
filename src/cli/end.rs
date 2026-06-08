use anyhow::{Context, Result, anyhow};
use console::style;
use git2::{PushOptions, Repository};

use crate::util::branch::{
  fetch_upstream_branch,
  find_local_of_upstream,
  get_current_branch_name,
  is_merged,
  switch,
};
use crate::util::branch_meta::BranchMeta;
use crate::util::display::trim_hash;
use crate::util::string::ToStrLossy;
use crate::util::{delete_config_section, get_push_callbacks};
use crate::{App, data, style};

const LONG_ABOUT: &str = r#"Safely deletes a feature branch, checking if it's merged into its base. If
currently checked-out, switches to the base branch."#;

const NO_BRANCH_MSG: &str = r#"No branch to delete! Either switch to a branch or specify one manually:
"feature end <BRANCH>""#;

const NO_BASE_MSG: &str = r#"Branch does not have a base! If this is meant to be a feature branch, specify
the base manually with the "--base <BRANCH>" option. If it's not a feature
branch, delete it normally with "git branch -d <BRANCH>"."#;

#[derive(clap::Args, Clone, Debug)]
#[command(
  about = "Ends a feature branch",
  long_about = LONG_ABOUT,
  disable_help_flag = true,
  disable_help_subcommand = true
)]
pub struct Args {
  /// Also delete the remote reference
  #[arg(short, long, value_name = "DELETE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  pub remote: Option<bool>,

  /// Delete branch without checking if it's merged
  #[arg(short, long)]
  pub force: bool,

  /// Skip automatic fetch of base branch
  #[arg(short = 'F', long, value_name = "SKIP", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  pub no_fetch: Option<bool>,

  /// The branch to treat as its base
  #[arg(long)]
  pub base: Option<String>,

  /// The branch to end. Defaults to the current branch.
  pub branch: Option<String>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let config = state.repo.config()?.snapshot()?;

    let branch = match &self.branch {
      Some(name) => BranchMeta::from_name_dwim(&state.repo, name)?,
      None => BranchMeta::current(&state.repo)?,
    }
    .context(NO_BRANCH_MSG)?;

    let base = match &self.base {
      Some(name) => BranchMeta::from_name_dwim(&state.repo, name)?,
      None => data::get_feature_base(&state.repo, branch.name())?,
    };

    let Some(base) = base else {
      return Err(anyhow!(NO_BASE_MSG));
    };

    let skip_fetch = match self.no_fetch {
      Some(it) => it,
      None => !data::get_feature_autofetch(&config)?,
    };

    // check if it's merged before deleting (unless --force)
    if !self.force {
      if base.is_remote() && !skip_fetch {
        // fetch latest base
        fetch_upstream_branch(&state.repo, &base)?;
        println!("{}", style!("Fetched {}", base.name()).dim());
      }

      let is_merged = is_merged(
        &state.repo,
        &branch.resolve(&state.repo)?,
        &base.resolve(&state.repo)?,
      )?;

      if !is_merged {
        return Err(anyhow!(
          "{} is not merged into {}",
          branch.name(),
          base.name()
        ));
      }
    }

    // if we're on the branch being deleted, we have to switch off
    let current = get_current_branch_name(&state.repo)?;
    if current.is_some_and(|it| it == branch.name()) {
      if base.is_remote() {
        let base_local = find_local_of_upstream(&state.repo, &base)?
          .with_context(|| format!("Failed to find local branch tracking {}", base.refname()))?;

        let meta = BranchMeta::from_branch(&base_local)?;
        switch(&state.repo, &meta)?;
        println!("{} to {}", style("Switched").green(), meta.name());
      } else {
        switch(&state.repo, &base)?;
        println!("{} to {}", style("Switched").green(), base.name());
      }
    }

    let delete_remote = match self.remote {
      Some(it) => it,
      None => data::get_end_remote(&config)?,
    };

    // begin actual deletions
    if delete_remote && let Err(e) = delete_upstream(&state.repo, &branch) {
      eprintln!("Failed to delete upstream: {}", e);
    }

    // delete local branch
    let mut branch_ref = branch.resolve(&state.repo)?;
    let branch_tip = branch_ref.peel_to_commit()?;
    branch_ref.delete()?;
    println!(
      "{} {} {}",
      style("Deleted").red(),
      branch.name(),
      style!("(was {})", trim_hash(&branch_tip)?).dim()
    );

    // delete branch's config
    let key = format!("branch.{}", branch.name());
    delete_config_section(&key)?;

    Ok(())
  }
}

fn delete_upstream(repo: &Repository, branch: &BranchMeta) -> Result<()> {
  if let Some(mut upstream) = branch.upstream(repo)? {
    let tip = upstream.get().peel_to_commit()?;
    let upstream_meta = BranchMeta::from_branch(&upstream)?;

    // delete from remote: `push :upstream_refname`
    let refspec = {
      let name_on_remote = repo.branch_upstream_merge(branch.refname())?;
      format!(":{}", name_on_remote.to_str_lossy())
    };

    let remote_name = repo.branch_upstream_remote(branch.refname())?;
    let remote_name = remote_name.to_str_lossy();
    let mut remote = repo.find_remote(&remote_name)?;

    let mut opts = PushOptions::new();
    opts.remote_callbacks(get_push_callbacks(repo));

    // the callbacks handle printing relevant output
    remote.push(&[&refspec], Some(&mut opts))?;

    // delete local copy
    upstream.delete()?;
    println!(
      "{} {} {}",
      style("Deleted").red(),
      upstream_meta.name(),
      style!("(was {})", trim_hash(&tip)?).dim()
    );
  }

  Ok(())
}
