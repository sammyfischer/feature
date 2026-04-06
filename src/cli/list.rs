use anyhow::{Context, Result, anyhow};
use console::{Term, measure_text_width, pad_str, style, truncate_str};
use git2::{Branch, ErrorCode, Repository};

use crate::cli::{get_current_branch, get_term_width};
use crate::{data, open_repo};

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
    col_widths.hash = 7; // every hash is 7 characters

    for (branch, _) in branches.flatten() {
      let row = self.build_branch_line(&repo, &branch);
      match row {
        Ok(row) => {
          let branch_width = row.branch.len();
          let upstream_width = row.upstream.len();
          let ab_upstream_width = measure_text_width(&row.ab_upstream);
          let base_width = row.base.len();
          let ab_base_width = measure_text_width(&row.ab_base);

          col_widths.branch = col_widths.branch.max(branch_width);
          col_widths.upstream = col_widths.upstream.max(upstream_width);
          col_widths.ab_upstream = col_widths.ab_upstream.max(ab_upstream_width);
          col_widths.base = col_widths.base.max(base_width);
          col_widths.ab_base = col_widths.ab_base.max(ab_base_width);

          rows.push(row);
        }
        Err(e) => eprintln!("{}", e),
      }
    }

    let current = get_current_branch(&repo).ok();
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

        let hash = fix_width(&row.hash, 7, trunc_tail);
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

      if Term::stdout().is_term() {
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

    let branch_name = branch
      .name()
      .context("Failed to get branch name")?
      .expect("Branch name is not valid utf-8");

    row.branch = branch_name.to_string();

    let branch_commit = branch.get().peel_to_commit().context(format!(
      "Failed to get commit pointed to by {}",
      branch_name
    ))?;

    row.hash = branch_commit.id().to_string()[..7].to_string();

    match branch.upstream() {
      Ok(it) => {
        let upstream_name = it
          .name()
          .context("Failed to get upstream name")?
          .expect("Upstream name is not valid utf-8")
          .to_string();

        row.upstream = upstream_name.clone();

        let upstream_commit = it.get().peel_to_commit().context(format!(
          "Failed to get commit pointed to by {}",
          upstream_name
        ))?;

        let (ahead, behind) = repo
          .graph_ahead_behind(branch_commit.id(), upstream_commit.id())
          .context(format!(
            "Faled to calculate ahead/behind against upstream {}",
            upstream_name
          ))?;

        // handling colors here bc we don't need to do any width calculations on this
        row.ab_upstream = format!(
          "{} {}",
          style(format!("+{}", ahead)).green(),
          style(format!("-{}", behind)).red()
        );
      }
      Err(e) if e.code() == ErrorCode::NotFound => {}
      Err(e) => return Err(anyhow!(e)),
    }

    let base_name = data::get_feature_base(&data::git_config(repo)?, branch_name);
    if let Some(base_name) = base_name {
      row.base = base_name
        .strip_prefix("refs/remotes/")
        .unwrap_or(&base_name)
        .to_string();

      let base = repo.revparse_single(&base_name).context(format!(
        "Failed to get reference to base branch {}",
        base_name
      ))?;

      let base_commit = base
        .peel_to_commit()
        .context(format!("Failed to get commit pointed to by {}", base_name))?;

      let (ahead, behind) = repo
        .graph_ahead_behind(branch_commit.id(), base_commit.id())
        .context(format!(
          "Failed to calculate ahead/behind against base {}",
          base_name
        ))?;

      row.ab_base = format!(
        "{} {}",
        style(format!("+{}", ahead)).green(),
        style(format!("-{}", behind)).red()
      );
    }

    let subject = branch_commit
      .summary()
      .expect("Commit message should be valid utf-8");

    row.subject = subject.to_string();

    Ok(row)
  }
}

#[inline(always)]
fn fix_width(s: &str, width: usize, tail: &str) -> String {
  pad_str(s, width, console::Alignment::Left, Some(tail)).to_string()
}
