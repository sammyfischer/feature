use anyhow::{Context, Result, anyhow};
use console::{Alignment, measure_text_width, pad_str, style, truncate_str};
use git2::{Branch, Repository};

use crate::util::branch::{
  branch_to_commit,
  branch_to_name,
  get_ahead_behind,
  get_current_branch_name,
  get_upstream,
  get_worktree_branch_names,
};
use crate::util::display::{display_plus_minus, trim_hash};
use crate::util::term::{get_term_width, is_term};
use crate::{App, data, lossy};

const LONG_ABOUT: &str = r#"Lists all branches. The format is similar to "git branch -vv"."#;

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
  #[inline]
  fn new() -> Self {
    Self::default()
  }

  #[inline]
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
  #[inline]
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
  #[arg(short = 'H', long, value_name = "HIDE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  pub no_hash: Option<bool>,

  /// Hides upstream branch column
  #[arg(short = 'U', long, value_name = "HIDE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  pub no_upstream: Option<bool>,

  /// Hides base branch column
  #[arg(short = 'B', long, value_name = "HIDE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  pub no_base: Option<bool>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let branches = state
      .repo
      .branches(Some(git2::BranchType::Local))
      .context("Failed to get list of branches")?;

    let mut rows: Vec<Row> = vec![Row::header()];

    // the width each column will be
    let mut col_widths = Row::header().widths();

    for (branch, _) in branches.flatten() {
      let row = self.build_branch_line(&state.repo, &branch);
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

    let current = get_current_branch_name(&state.repo)?;
    let wt_branches = get_worktree_branch_names(&state.repo)?;
    let max_widths = Widths::max();
    let line_tail = style("…").dim().to_string();
    let trunc_tail = "…";
    let term_width = get_term_width();

    use std::fmt::Write;
    let mut buf = String::with_capacity(200);

    for (i, row) in rows.iter().enumerate() {
      buf.clear();

      'branch: {
        let branch = fix_width(
          &row.branch,
          col_widths.branch.min(max_widths.branch),
          trunc_tail,
        );

        if i == 0 {
          write!(buf, "{}", &style(branch).bold().to_string())?;
          break 'branch;
        }

        if current.as_ref().is_some_and(|it| it == &row.branch) {
          write!(buf, "{}", style(&branch).green())?;
        } else if wt_branches.contains(&row.branch) {
          write!(buf, "{}", style(&branch).cyan())?;
        } else {
          write!(buf, "{}", &branch)?;
        }
      }

      'hash: {
        if self.no_hash.unwrap_or(!state.config.list.hash) {
          break 'hash;
        }

        let hash = fix_width(&row.hash, col_widths.hash, trunc_tail);

        if i == 0 {
          write!(buf, " {}", style(&hash).bold().yellow())?;
        } else {
          write!(buf, " {}", style(&hash).yellow())?;
        }
      }

      'upstream: {
        if self.no_upstream.unwrap_or(!state.config.list.upstream) {
          break 'upstream;
        }

        let upstream = fix_width(
          &row.upstream,
          col_widths.upstream.min(max_widths.upstream),
          trunc_tail,
        );
        let ab = fix_width(&row.ab_upstream, col_widths.ab_upstream, trunc_tail);

        if i == 0 {
          write!(buf, " {} {}", style(&upstream).bold().blue(), &ab)?;
        } else {
          write!(buf, " {} {}", style(&upstream).blue(), &ab)?;
        }
      }

      'base: {
        if self.no_base.unwrap_or(!state.config.list.base) {
          break 'base;
        }

        let base = fix_width(&row.base, col_widths.base.min(max_widths.base), trunc_tail);
        let ab = fix_width(&row.ab_base, col_widths.ab_base, trunc_tail);

        if i == 0 {
          write!(buf, " {} {}", style(&base).bold().magenta(), &ab)?;
        } else {
          write!(buf, " {} {}", style(&base).magenta(), &ab)?;
        }
      }

      if i == 0 {
        write!(buf, " {}", style(&row.subject).bold())?;
      } else {
        write!(buf, " {}", &row.subject)?;
      }

      if is_term() {
        println!("{}", truncate_str(&buf, term_width, &line_tail));
      } else {
        println!("{}", &buf);
      }
    }

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
      let (a, b) = get_ahead_behind(repo, branch.get(), upstream.get()).with_context(|| {
        format!(
          "Failed to get ahead/behind between {} and {}",
          &branch_name, &upstream_name
        )
      })?;

      row.upstream = upstream_name.to_string();
      row.ab_upstream = display_plus_minus(a, b);
    }

    let base = data::get_feature_base(repo, &branch_name)?;
    if let Some(base) = base {
      row.base = base.name().to_string();

      let (a, b) =
        get_ahead_behind(repo, branch.get(), &base.resolve(repo)?).with_context(|| {
          format!(
            "Failed to get ahead/behind between {} and {}",
            &branch_name,
            base.name()
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

#[inline]
fn fix_width(s: &str, width: usize, tail: &str) -> String {
  pad_str(s, width, Alignment::Left, Some(tail)).to_string()
}
