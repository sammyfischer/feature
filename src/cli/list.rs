use anyhow::{Context, Result};
use console::{Alignment, measure_text_width, pad_str, style, truncate_str};
use git2::{Branch, Branches, Repository};

use crate::util::branch::{
  get_ahead_behind, get_current_branch_name, get_upstream, get_worktree_branch_names,
};
use crate::util::display::{display_plus_minus, trim_hash};
use crate::util::string::{ToStrLossy, ToStrLossyOwned};
use crate::util::term::{get_term_width, is_term};
use crate::{App, data};

const LONG_ABOUT: &str = r#"Lists all branches. The format is similar to "git branch -vv"."#;

#[derive(clap::Args, Clone, Debug)]
#[command(
  visible_alias = "ls",
  about = "Lists branches",
  long_about = LONG_ABOUT,
  disable_help_flag = true,
  disable_help_subcommand = true
)]
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

  /// Hides feature subprojects from output
  #[arg(short = 'P', long, value_name = "HIDE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  pub no_projects: Option<bool>,

  /// Hides git submodules from output
  #[arg(short = 'M', long, value_name = "HIDE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  pub no_modules: Option<bool>,
}

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

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let config = state.repo.config()?.snapshot()?;
    let branches = state
      .repo
      .branches(Some(git2::BranchType::Local))
      .context("Failed to get list of branches")?;

    let (rows, widths) = self.build_table(&state.repo, branches)?;
    self.print_table(&state.repo, &rows, widths)?;

    let hide_projects = match self.no_projects {
      Some(it) => it,
      None => !data::get_feature_show_projects(&config)?,
    };

    if !hide_projects {
      for (name, project) in &state.config.projects {
        let repo = Repository::open(&project.path)?;
        let branches = repo.branches(Some(git2::BranchType::Local))?;

        println!("\n{} {}", style("Project").bold(), style(name).cyan());
        let (rows, widths) = self.build_table(&repo, branches)?;
        self.print_table(&repo, &rows, widths)?;
      }
    }

    let hide_modules = match self.no_modules {
      Some(it) => it,
      None => !data::get_feature_show_modules(&config)?,
    };

    if !hide_modules {
      let modules = state.repo.submodules()?;
      for module in modules {
        let name = module.name_bytes().to_str_lossy();
        let repo = module.open()?;
        let branches = repo.branches(Some(git2::BranchType::Local))?;

        println!("\n{} {}", style("Module").bold(), style(name).cyan());
        let (rows, widths) = self.build_table(&repo, branches)?;
        self.print_table(&repo, &rows, widths)?;
      }
    }

    Ok(())
  }

  fn build_row(&self, repo: &Repository, branch: &Branch) -> Result<Row> {
    let mut row = Row::new();
    let branch_name = branch.name_bytes()?.to_str_lossy();
    row.branch = branch_name.to_string();

    let branch_commit = branch.get().peel_to_commit()?;
    row.hash = trim_hash(&branch_commit)?;

    if let Some(upstream) = get_upstream(branch)? {
      let upstream_name = upstream.name_bytes()?.to_str_lossy();
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

    row.subject = branch_commit
      .summary_bytes()
      .context("Commit has no summary")?
      .to_str_lossy_owned();

    Ok(row)
  }

  fn build_table(&self, repo: &Repository, branches: Branches) -> Result<(Vec<Row>, Widths)> {
    let mut rows: Vec<Row> = vec![Row::header()];
    let mut widths = Row::header().widths();

    for (branch, _) in branches.flatten() {
      let row = self.build_row(repo, &branch);
      match row {
        Ok(row) => {
          let branch_width = row.branch.len();
          let hash_width = row.hash.len();
          let upstream_width = row.upstream.len();
          let ab_upstream_width = measure_text_width(&row.ab_upstream);
          let base_width = row.base.len();
          let ab_base_width = measure_text_width(&row.ab_base);

          widths.branch = widths.branch.max(branch_width);
          widths.hash = widths.hash.max(hash_width);
          widths.upstream = widths.upstream.max(upstream_width);
          widths.ab_upstream = widths.ab_upstream.max(ab_upstream_width);
          widths.base = widths.base.max(base_width);
          widths.ab_base = widths.ab_base.max(ab_base_width);

          rows.push(row);
        }
        Err(e) => eprintln!("{}", e),
      }
    }

    Ok((rows, widths))
  }

  fn print_table(&self, repo: &Repository, rows: &[Row], widths: Widths) -> Result<()> {
    let current = get_current_branch_name(repo)?;
    let wt_branches = get_worktree_branch_names(repo)?;
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
          widths.branch.min(max_widths.branch),
          trunc_tail,
        );

        if i == 0 {
          write!(buf, "{}", &style(branch).bold().green().to_string())?;
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

      let config = repo.config()?.snapshot()?;

      'hash: {
        if self.no_hash.unwrap_or(!data::get_list_hash(&config)?) {
          break 'hash;
        }

        let hash = fix_width(&row.hash, widths.hash, trunc_tail);

        if i == 0 {
          write!(buf, " {}", style(&hash).bold().yellow())?;
        } else {
          write!(buf, " {}", style(&hash).yellow())?;
        }
      }

      'upstream: {
        if self
          .no_upstream
          .unwrap_or(!data::get_list_upstream(&config)?)
        {
          break 'upstream;
        }

        let upstream = fix_width(
          &row.upstream,
          widths.upstream.min(max_widths.upstream),
          trunc_tail,
        );
        let ab = fix_width(&row.ab_upstream, widths.ab_upstream, trunc_tail);

        if i == 0 {
          write!(buf, " {} {}", style(&upstream).bold().blue(), &ab)?;
        } else {
          write!(buf, " {} {}", style(&upstream).blue(), &ab)?;
        }
      }

      'base: {
        if self.no_base.unwrap_or(!data::get_list_base(&config)?) {
          break 'base;
        }

        let base = fix_width(&row.base, widths.base.min(max_widths.base), trunc_tail);
        let ab = fix_width(&row.ab_base, widths.ab_base, trunc_tail);

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
}

#[inline]
fn fix_width(s: &str, width: usize, tail: &str) -> String {
  pad_str(s, width, Alignment::Left, Some(tail)).to_string()
}
