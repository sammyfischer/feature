use anyhow::{Context, Result};
use console::{measure_text_width, pad_str, style, truncate_str};
use git2::DiffOptions;

use crate::cli::Cli;
use crate::util::branch::{
  branch_to_name,
  get_ahead_behind,
  get_head,
  get_upstream,
  name_to_branch,
  name_to_remote_branch,
};
use crate::util::display::{
  display_diff_summary,
  display_hash,
  display_plus_minus,
  display_signature,
};
use crate::util::get_signature;
use crate::util::term::{get_term_width, is_term};
use crate::{data, lossy, open_repo};

#[derive(clap::Args, Clone, Debug)]
#[command(
  visible_alias = "st",
  about = "View current status (current branch, author info, changes)"
)]
pub struct Args {
  /// Hides untracked files from output
  #[arg(short = 'u', long)]
  pub hide_untracked: bool,
}

impl Args {
  pub fn run(&self, cli: &Cli) -> Result<()> {
    let repo = open_repo!();
    let head = get_head(&repo)?;
    let mut branch_name = None;

    let first_line = match &head {
      // there are commits in the repo
      Some(head) => {
        let commit = head
          .peel_to_commit()
          .context("Failed to get commit at HEAD")?;

        // display branch name or detached head indicator
        let display_branch = if head.is_branch() {
          let name = lossy!(head.shorthand_bytes()).to_string();
          branch_name = Some(name.clone());
          format!("On {}", style(&name).green())
        } else {
          style("Detached HEAD").red().to_string()
        };

        format!(
          "{} -> {} {}",
          display_branch,
          display_hash(&commit.id()),
          lossy!(
            commit
              .summary_bytes()
              .context("Failed to get commit message")?
          )
        )
      }

      // head points to nothing, no commits in repo
      None => style("No commits yet").dim().to_string(),
    };

    // end first line
    if is_term() {
      println!(
        "{}",
        truncate_str(
          &first_line,
          get_term_width(),
          &style("\u{2026}").dim().to_string()
        )
      );
    } else {
      println!("{}", first_line);
    }

    // upstream and base ahead/behind if we're on a branch
    if head.as_ref().is_some_and(|it| it.is_branch()) {
      let branch_name =
        branch_name.context("Branch name should exist when HEAD is not detached")?;
      let branch = name_to_branch(&repo, &branch_name)?;

      let mut rows: Vec<[String; 2]> = Vec::new();
      // the label is either "Upstream" or "Base", these are printed with alignment so the branch
      // names are lined up
      let mut label_width = 0usize;

      // upstream row
      let upstream = get_upstream(&branch)?;
      if let Some(upstream) = upstream {
        let upstream_name = branch_to_name(&upstream)?;
        let (a, b) = get_ahead_behind(&repo, &branch, &upstream)
          .context("Failed to get ahead/behind for upstream")?;

        let row = [
          style("Upstream").blue().to_string(),
          format!(
            "{}{} {}{}",
            style("[").dim(),
            style(&upstream_name),
            display_plus_minus(a, b),
            style("]").dim(),
          ),
        ];
        label_width = measure_text_width(&row[0]);
        rows.push(row);
      }

      // base row
      let base_name = data::get_short_feature_base(&data::git_config(&repo)?, &branch_name);
      if let Some(base_name) = base_name {
        let base = name_to_remote_branch(&repo, &base_name)?;
        let (a, b) =
          get_ahead_behind(&repo, &branch, &base).context("Failed to get ahead/behind for base")?;

        let row = [
          style("Base").magenta().to_string(),
          format!(
            "{}{} {}{}",
            style("[").dim(),
            style(&base_name),
            display_plus_minus(a, b),
            style("]").dim(),
          ),
        ];
        label_width = label_width.max(measure_text_width(&row[0]));
        rows.push(row);
      }

      // print with everything after the row label aligned
      for row in rows {
        println!(
          "  {} {}",
          pad_str(&row[0], label_width, console::Alignment::Left, None),
          &row[1]
        );
      }
    }

    // signature/author info
    println!("{}", display_signature(get_signature(&repo)?.as_ref()));

    // get current tree to diff from
    let tree = match &head {
      Some(head) => {
        let commit = head.peel_to_commit()?;
        Some(commit.tree()?)
      }
      None => None,
    };

    // staged changes
    let diff = repo
      .diff_tree_to_index(tree.as_ref(), None, None)
      .context("Failed to get staged chagnes")?;

    let stats = diff.stats().context("Failed to get staged changes")?;

    // if there are changes to print
    if stats.files_changed() != 0 {
      println!();
      print!("{} - ", style("Staged").green());
      println!("{}", match display_diff_summary(&diff) {
        Ok(it) => it,
        Err(_) => "Failed to get summary of staged changes".to_string(),
      });
    }

    // unstaged changes
    let mut opts = if self.hide_untracked || cli.config.hide_untracked {
      None
    } else {
      let mut opts = DiffOptions::new();
      opts.include_untracked(true);
      Some(opts)
    };

    let diff = repo
      .diff_index_to_workdir(None, opts.as_mut())
      .context("Failed to get unstaged changes")?;

    let stats = diff.stats().context("Failed to get unstaged changes")?;

    // if there are changes to print
    if stats.files_changed() != 0 {
      println!();
      print!("{} - ", style("Unstaged").red());
      println!("{}", match display_diff_summary(&diff) {
        Ok(it) => it,
        Err(_) => "Failed to get summary of unstaged changes".to_string(),
      });
    }

    Ok(())
  }
}
