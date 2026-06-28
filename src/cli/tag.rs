use anyhow::{Context, Result};
use clap::ValueHint;
use console::style;
use git2::{ErrorCode, PushOptions};

use crate::util::display::{
  DisplayCommitMessageLevel,
  DisplayCommitOptions,
  DisplayTimeOptions,
  display_tag,
};
use crate::util::string::ToStrLossyOwned;
use crate::util::tag::SemverTag;
use crate::util::{PushOutput, get_push_callbacks};
use crate::{App, data};

const LONG_ABOUT: &str = r#"Creates and pushes a semver tag.

The version string specified may contain a
leading v, but must have 3 numbers separated by a dot. For example, "v1.0.0"
and "1.0.0" are accepted and equivalent.

When no "--message" is specified, this creates a lightweight tag. With a
message, this creates an annotated tag."#;

#[derive(clap::Args, Clone, Debug)]
#[command(
  about = "Create and push a semver tag",
  long_about = LONG_ABOUT,
  disable_help_subcommand = true
)]
pub struct Args {
  /// Where to put the tag
  #[arg(long, value_name = "REVISION", value_hint = ValueHint::Other)]
  at: Option<String>,

  /// The remote to push to
  #[arg(short, long)]
  remote: Option<String>,

  /// Whether to push the tag after creating
  #[arg(long, default_value = "true")]
  push: bool,

  /// The message for the tag. This implicitly makes an annotated tag. Takes
  /// the entire message as a single string.
  #[arg(short, long)]
  message: Option<String>,

  /// The semver. The leading 'v' is optional
  version: String,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo = &state.repo;
    let head = repo.head()?;

    let (target, target_name) = match &self.at {
      Some(rev) => {
        let obj = repo.revparse_single(rev)?;

        // if the user typed a ref shorthand, this will find a suitable name. if not,
        // default to short hash
        let name = match repo.resolve_reference_from_short_name(rev) {
          Ok(reference) => reference.shorthand_bytes().to_str_lossy_owned(),
          Err(_) => obj.short_id()?.to_str_lossy_owned(),
        };
        (obj, name)
      }

      None => {
        let obj = head.peel_to_commit()?.into_object();
        let name = head.shorthand_bytes().to_str_lossy_owned();
        (obj, name)
      }
    };

    let mut v = &self.version[..];
    if v.starts_with('v') {
      v = &v[1..];
    }

    let (major, minor, patch) = SemverTag::parse(v)?;
    let name = format!("v{}.{}.{}", major, minor, patch);
    let refname = format!("refs/tags/{}", name);

    if let Some(msg) = &self.message {
      // annotated tag
      let sig = repo.signature()?;
      let tag_id = repo.tag(&name, &target, &sig, msg, false)?;
      let tag = repo.find_tag(tag_id)?;

      println!(
        "{} tag {} at {}",
        style("Created").green(),
        style(&name).cyan(),
        &target_name
      );

      println!(
        "{}",
        display_tag(&tag, &DisplayCommitOptions {
          time: DisplayTimeOptions {
            // tag was just created, relative is not useful
            relative: false,
            fmt: data::get_format_date(&repo.config()?)?,
          },
          message: DisplayCommitMessageLevel::Full,
        })?
      );
    } else {
      // lightweight tag
      repo.tag_lightweight(&name, &target, false)?;
      println!(
        "{} tag {} at {}",
        style("Created").green(),
        style(&name).cyan(),
        &target_name
      );
    }

    // push the tag
    if self.push {
      let remote_name = self.remote.as_ref().unwrap_or(&state.config.default_remote);

      match repo.find_remote(remote_name) {
        Ok(mut remote) => {
          let mut output = PushOutput::new();
          {
            let mut opts = PushOptions::new();
            opts.remote_callbacks(get_push_callbacks(repo, &mut output)?);
            remote.push(&[&refname], Some(&mut opts))?;
          }
          output.print();

          println!(
            "{} tag {} to {}",
            style("Pushed").green(),
            style(&name).cyan(),
            style(remote_name).blue()
          );
        }

        Err(e) if e.code() == ErrorCode::NotFound => {}
        Err(e) => return Err(e).context("Failed to find remote"),
      };
    }

    Ok(())
  }
}
