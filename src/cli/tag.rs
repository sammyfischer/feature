use anyhow::{Result, anyhow};
use clap::ValueHint;
use console::style;
use git2::Tag;

use crate::App;
use crate::cli::display::commit::DisplayCommitOptions;
use crate::cli::display::time::{DisplayTimeOptions, display_time};
use crate::cli::display::{display_hash, display_signature};
use crate::cli::push::{configure_and_push, display_push_status};
use crate::core::NotFoundExt;
use crate::core::string::{ToStrLossy, ToStrLossyOwned};
use crate::core::tag::SemverTag;
use crate::core::user_config::{CommitMessageLevel, UserConfig};

const LONG_ABOUT: &str = r#"Creates and pushes a semver tag.

The version string specified may contain a
leading v, but must have 3 numbers separated by a dot. For example, "v1.0.0"
and "1.0.0" are accepted and equivalent.

When no "--message" is specified, this creates a lightweight tag. With a
message, this creates an annotated tag."#;

const NOT_ANNOTATED_MSG: &str = r#"This repository requires annotated tags!
Specify a message with "-m" to annotate."#;

#[derive(clap::Args, Clone, Debug)]
#[command(
  about = "Create and push a semver tag",
  long_about = LONG_ABOUT,
  disable_help_subcommand = true
)]
pub struct TagArgs {
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

impl TagArgs {
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
            fmt: UserConfig::new(repo)?.format_date()?,
          },
          message: CommitMessageLevel::Full,
        })?
      );
    } else {
      if state.config.tag.require_annotated {
        return Err(anyhow!(NOT_ANNOTATED_MSG));
      }

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

      if let Some(mut remote) = repo.find_remote(remote_name).not_found_ok()? {
        let status = configure_and_push(&mut remote, &refname)?;
        println!("{}", display_push_status(repo, status)?);

        println!(
          "{} tag {} to {}",
          style("Pushed").green(),
          style(&name).cyan(),
          style(remote_name).blue()
        );
      };
    }

    Ok(())
  }
}

/// Displays a tag object in a format similar to a commit. Reuses
/// [DisplayCommitOptions] for convenience.
pub fn display_tag(tag: &Tag, options: &DisplayCommitOptions) -> Result<String> {
  use std::fmt::Write;
  // around 60 chars for hash/time/author, another 80 for message (most of the
  // time this will only be a subject line)
  let mut out = String::with_capacity(140);

  // hash
  write!(out, "{}", display_hash(tag.as_object())?)?;

  if let Some(tagger) = tag.tagger().as_ref() {
    // timestamp
    write!(
      out,
      " {}",
      style(display_time(&tagger.when(), &options.time)?).magenta()
    )?;

    // author
    write!(out, " by {}", display_signature(Some(tagger)))?;
  }

  if let Some(msg) = tag.message_bytes() {
    match options.message {
      CommitMessageLevel::None => {}

      // there is no subject line for tags. could just parse it out myself but I'd rather not
      // support it if it's non standard
      CommitMessageLevel::Subject | CommitMessageLevel::Full => {
        // write each line tabbed by 2 spaces
        writeln!(out)?;
        for line in msg.to_str_lossy().lines() {
          write!(out, "\n  {}", line)?;
        }
      }
    };
  }

  Ok(out)
}
