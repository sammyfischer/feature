use anyhow::{Context, Result, anyhow};
use console::style;
use git2::{ErrorCode, PushOptions};

use crate::cli::Cli;
use crate::util::branch::get_current_branch_name;
use crate::util::get_remote_callbacks;
use crate::{lossy, open_repo};

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Pushes a branch to remote, setting upstream automatically")]
pub struct Args {
  /// Force push
  #[arg(short, long)]
  force: bool,

  /// Which remote to push to, if no upstream is already set
  #[arg(short, long)]
  remote: Option<String>,

  /// The name of the upstream branch, if no upstream is already set
  #[arg(short, long)]
  upstream: Option<String>,
}

impl Args {
  pub fn run(&self, cli: &Cli) -> Result<()> {
    let repo = open_repo!();
    let branch_name =
      get_current_branch_name(&repo)?.context("Not currently on a branch! Nothing to push.")?;

    // allow pushing bases, but as fast-forward only. the remote can still choose to reject
    if cli.config.bases.contains(&branch_name) && self.force {
      return Err(anyhow!("Cannot force push a base branch"));
    }

    // same for protected branches
    if cli.config.protect.contains(&branch_name) && self.force {
      return Err(anyhow!("Cannot force push a protected branch"));
    }

    let mut branch = repo
      .find_branch(&branch_name, git2::BranchType::Local)
      .with_context(|| format!("Failed to get reference to branch {}", branch_name))?;
    let branch_refname = lossy!(&branch.get().name_bytes());

    let mut should_set_upstream = false;
    let (upstream_name, remote_name) = match branch.upstream() {
      Ok(it) => {
        let mut name = lossy!(
          &it
            .name_bytes()
            .context("Failed to get existing upstream name")?
        )
        .to_string();

        // parse out the remote name (before the first slash) and the upstream name (the rest)
        let split_at = name
          .find('/')
          .context("Upstream name has an invalid format")?;

        let upstream_name = name.split_off(split_at).trim_prefix('/').to_string();
        let remote_name = name;
        (upstream_name, remote_name)
      }

      // no upstream set, use flags or defaults
      Err(e) if e.code() == ErrorCode::NotFound => {
        should_set_upstream = true;
        (
          self.upstream.as_ref().unwrap_or(&branch_name).to_string(),
          self
            .remote
            .as_ref()
            .unwrap_or(&cli.config.default_remote)
            .clone(),
        )
      }

      Err(e) => return Err(e.into()),
    };

    let mut remote = repo
      .find_remote(&remote_name)
      .with_context(|| format!("Failed to get reference to remote {}", remote_name))?;

    let mut opts = PushOptions::new();
    let mut cbs = get_remote_callbacks();

    // print error if push fails
    cbs.push_update_reference(|refname, status| {
      // a status of Some means push was rejected
      if let Some(msg) = status {
        eprintln!(
          "{} to {} {}: {}",
          style("Push").red(),
          refname,
          style("failed").red(),
          msg
        );
        return Err(git2::Error::from_str(msg));
      }
      Ok(())
    });

    opts.remote_callbacks(cbs);

    // build the refspec
    let mut refspec = String::new();
    if self.force {
      refspec.push('+');
    }
    refspec.push_str(&branch_refname);
    refspec.push(':');
    refspec.push_str("refs/heads/");
    refspec.push_str(&upstream_name);

    remote
      .push(&[&refspec], Some(&mut opts))
      .context("Failed to push")?;

    let mut out = format!(
      "{} {} to {}",
      if self.force {
        style("Force-pushed").yellow()
      } else {
        style("Pushed").green()
      },
      style(&branch_name).blue(),
      style(&remote_name).magenta()
    );

    // set upstream if not already
    if should_set_upstream {
      let set_upstream_to = format!("{}/{}", &remote_name, &upstream_name);

      out.push_str(
        &style(format!(" (tracking {})", set_upstream_to))
          .dim()
          .to_string(),
      );

      branch
        .set_upstream(Some(&set_upstream_to))
        .context("Failed to set upstream tracking branch")?;
    }

    println!("{}", out);
    Ok(())
  }
}
