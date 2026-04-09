use anyhow::{Context, Result, anyhow};
use console::{Alignment, measure_text_width, pad_str, style, truncate_str};
use git2::{Branch, Repository};

use crate::util::branch::{
  branch_to_commit,
  branch_to_name,
  get_ahead_behind,
  get_current_branch_name,
  get_upstream,
  name_to_remote_branch,
};
use crate::util::display::{display_plus_minus, trim_hash};
use crate::util::term::{get_term_width, is_term};
use crate::{data, lossy, open_repo};

const LONG_ABOUT: &str = r"Lists all branches.

The default format is similar to `git branch -vv`. Formats can be specified
with a template string.

List the template replacements here.";

#[derive(Default)]
struct Row {
  branch: String,
  hash: String,
  upstream: String,
  ab_upstream: String,
  base: String,
  ab_base: String,
  subject: String,
}

impl Row {
  #[inline(always)]
  fn new() -> Self {
    Self::default()
  }

  #[inline(always)]
  fn header() -> Self {
    Self {
      branch: "Branch".into(),
      hash: "Hash".into(),
      upstream: "Upstream".into(),
      ab_upstream: "".into(),
      base: "Base".into(),
      ab_base: "".into(),
      subject: "Message".into(),
    }
  }

  fn widths(&self) -> Widths {
    Widths {
      branch: self.branch.len(),
      hash: self.hash.len(),
      upstream: self.upstream.len(),
      ab_upstream: self.ab_upstream.len(),
      base: self.base.len(),
      ab_base: self.ab_base.len(),
    }
  }
}

#[derive(Default)]
struct Widths {
  branch: usize,
  hash: usize,
  upstream: usize,
  ab_upstream: usize,
  base: usize,
  ab_base: usize,
}

impl Widths {
  #[inline(always)]
  fn max() -> Self {
    Self {
      branch: 30,
      hash: 7,
      upstream: 20,
      ab_upstream: usize::MAX, // shouldn't be truncated
      base: 20,
      ab_base: usize::MAX, // shouldn't be truncated
    }
  }
}

#[derive(clap::Args, Clone, Debug)]
#[command(visible_alias = "ls", about = "Lists branches", long_about = LONG_ABOUT)]
pub struct Args {
  /// Hides hash column
  #[arg(long)]
  pub no_hash: bool,

  /// Hides upstream branch column
  #[arg(long)]
  pub no_upstream: bool,

  /// Hides base branch column
  #[arg(long)]
  pub no_base: bool,
}

impl Args {
  pub fn run(&self) -> Result<()> {
    let repo = open_repo!();

    let branches = repo
      .branches(Some(git2::BranchType::Local))
      .context("Failed to get list of branches")?;

    let mut rows: Vec<Row> = vec![Row::header()];

    // the width each column will be
    let mut col_widths = Row::header().widths();

    for (branch, _) in branches.flatten() {
      let row = self.build_branch_line(&repo, &branch);
      match row {
        Ok(row) => {
          let branch_width = row.branch.len();
          let hash_width = row.hash.len();
          let upstream_width = row.upstream.len();
          let ab_upstream_width = measure_text_width(&row.ab_upstream);
          let base_width = row.base.len();
          let ab_base_width = measure_text_width(&row.ab_base);

          col_widths.branch = col_widths.branch.max(branch_width);
          col_widths.hash = col_widths.hash.max(hash_width);
          col_widths.upstream = col_widths.upstream.max(upstream_width);
          col_widths.ab_upstream = col_widths.ab_upstream.max(ab_upstream_width);
          col_widths.base = col_widths.base.max(base_width);
          col_widths.ab_base = col_widths.ab_base.max(ab_base_width);

          rows.push(row);
        }
        Err(e) => eprintln!("{}", e),
      }
    }

    let current = get_current_branch_name(&repo)?;
    let max_widths = Widths::max();
    let line_tail = style("\u{2026}").dim().to_string();
    let trunc_tail = "\u{2026}";
    let term_width = get_term_width();
    let mut out = String::new();

    for (i, row) in rows.iter().enumerate() {
      let mut line = String::new();

      'branch: {
        let branch = fix_width(
          &row.branch,
          col_widths.branch.min(max_widths.branch),
          trunc_tail,
        );

        if i == 0 {
          line.push_str(&style(branch).bold().to_string());
          break 'branch;
        }

        if current.as_ref().is_some_and(|it| it == &row.branch) {
          line.push_str(&style(&branch).green().to_string());
        } else {
          line.push_str(&branch);
        }
      }

      'hash: {
        if self.no_hash {
          break 'hash;
        }

        let hash = fix_width(&row.hash, col_widths.hash, trunc_tail);
        line.push(' ');

        if i == 0 {
          line.push_str(&style(hash).bold().yellow().to_string());
          break 'hash;
        }

        line.push_str(&style(hash).yellow().to_string());
      }

      'upstream: {
        if self.no_upstream {
          break 'upstream;
        }

        let upstream = fix_width(
          &row.upstream,
          col_widths.upstream.min(max_widths.upstream),
          trunc_tail,
        );
        let ab = fix_width(&row.ab_upstream, col_widths.ab_upstream, trunc_tail);

        line.push(' ');

        if i == 0 {
          line.push_str(&style(upstream).bold().blue().to_string());
          line.push(' ');
          line.push_str(&ab); // the header is just spaces, styles aren't needed
          break 'upstream;
        }

        line.push_str(&style(upstream).blue().to_string());
        line.push(' ');
        line.push_str(&ab);
      }

      'base: {
        if self.no_base {
          break 'base;
        }

        let base = fix_width(&row.base, col_widths.base.min(max_widths.base), trunc_tail);
        let ab = fix_width(&row.ab_base, col_widths.ab_base, trunc_tail);

        line.push(' ');

        if i == 0 {
          line.push_str(&style(base).bold().magenta().to_string());
          line.push(' ');
          line.push_str(&ab);
          break 'base;
        }

        line.push_str(&style(base).magenta().to_string());

        line.push(' ');
        line.push_str(&ab);
      }

      line.push(' ');
      if i == 0 {
        line.push_str(&style(&row.subject).bold().to_string());
      } else {
        line.push_str(&row.subject);
      }

      if is_term() {
        line = truncate_str(&line, term_width, &line_tail).to_string();
      }

      out.push_str(&line);
      out.push('\n');
    }

    print!("{}", out);
    Ok(())
  }

  fn build_branch_line(&self, repo: &Repository, branch: &Branch) -> Result<Row> {
    let mut row = Row::new();
    let branch_name = branch_to_name(branch)?;
    row.branch = branch_name.to_string();

    let branch_commit =
      branch_to_commit(branch)?.ok_or(anyhow!("Branch does not point to a commit"))?;

    row.hash = trim_hash(&branch_commit.id()).to_string();

    if let Some(upstream) = get_upstream(branch)? {
      let upstream_name = branch_to_name(&upstream)?;
      let (a, b) = get_ahead_behind(repo, branch, &upstream).with_context(|| {
        format!(
          "Failed to get ahead/behind between {} and {}",
          &branch_name, &upstream_name
        )
      })?;

      row.upstream = upstream_name.to_string();
      row.ab_upstream = display_plus_minus(a, b);
    }

    let base_name = data::get_short_feature_base(&data::git_config(repo)?, &branch_name);
    if let Some(base_name) = base_name {
      row.base = base_name.clone();

      let base = name_to_remote_branch(repo, &base_name)
        .with_context(|| format!("Failed to get reference to base branch {}", base_name))?;

      let (a, b) = get_ahead_behind(repo, branch, &base).with_context(|| {
        format!(
          "Failed to get ahead/behind between {} and {}",
          &branch_name, &base_name
        )
      })?;

      row.ab_base = display_plus_minus(a, b);
    }

    row.subject = lossy!(
      branch_commit
        .summary_bytes()
        .context("Commit has no summary")?
    )
    .to_string();

    Ok(row)
  }
}

#[inline(always)]
fn fix_width(s: &str, width: usize, tail: &str) -> String {
  pad_str(s, width, Alignment::Left, Some(tail)).to_string()
}
