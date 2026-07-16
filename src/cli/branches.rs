use std::thread;

use anyhow::{Context, Result};
use console::{measure_text_width, style, truncate_str};
use git2::{Branch, BranchType, Repository};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::cli::display::display_plus_minus;
use crate::cli::display::time::{DisplayTimeOptions, display_time};
use crate::cli::term::{get_term_width, is_term};
use crate::core::branch::{get_current_branch_name, get_worktree_branch_names};
use crate::core::string::{ToStrLossy, ToStrLossyOwned};
use crate::core::user_config::UserConfig;
use crate::core::{NotFoundExt, open_repo_from_dirs};
use crate::{App, style};

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Lists branches", disable_help_subcommand = true)]
pub struct Args {
  /// Hides feature projects from output
  #[arg(short = 'P', long, value_name = "HIDE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  no_projects: Option<bool>,

  /// Hides git submodules from output
  #[arg(short = 'M', long, value_name = "HIDE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  no_modules: Option<bool>,
}

#[derive(Debug, Default)]
struct Row {
  branch: String,
  upstream: Option<String>,
  base: Option<String>,
  commit: String,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo_dir = state.repo.path().to_owned();
    let work_dir = state.repo.workdir().to_owned();
    let app_config = &state.config;
    let user_config = UserConfig::new(&state.repo)?;

    let hide_projects = match self.no_projects {
      Some(it) => it,
      None => !user_config.show_projects()?,
    };

    let hide_modules = match self.no_modules {
      Some(it) => it,
      None => !user_config.show_modules()?,
    };
    let mod_names: Vec<_> = if hide_modules {
      Vec::new()
    } else {
      state
        .repo
        .submodules()?
        .iter()
        .map(|module| module.name_bytes().to_str_lossy_owned())
        .collect()
    };

    let out = thread::scope(|scope| -> Result<String> {
      let repo_thead = scope.spawn(|| -> Result<_> {
        let repo = open_repo_from_dirs(&repo_dir, work_dir)?;
        let user_config = UserConfig::new(&repo)?;

        let table = self.build_table(&repo, &user_config)?;
        self.display_table(table)
      });

      let proj_thread = scope.spawn(|| {
        if hide_projects {
          Vec::new()
        } else {
          app_config
            .projects
            .par_iter()
            .map(|(name, project)| -> Result<_> {
              let mut out = format!("\n{} {}", style("Project").bold(), style(name).cyan());
              let repo = Repository::open(&project.path)?;
              let user_config = UserConfig::new(&repo)?;

              let table = self.build_table(&repo, &user_config)?;
              out.push('\n');
              out.push_str(&self.display_table(table)?);
              Ok(out)
            })
            .collect()
        }
      });

      let mod_thread = scope.spawn(|| {
        if hide_modules {
          Vec::new()
        } else {
          mod_names
            .par_iter()
            .map(|name| -> Result<_> {
              let repo = open_repo_from_dirs(&repo_dir, work_dir)?;
              let user_config = UserConfig::new(&repo)?;
              let module = repo.find_submodule(name)?;
              let mod_repo = module.open()?;

              let mut out = format!("\n{} {}", style("Module").bold(), style(name).cyan());
              let table = self.build_table(&mod_repo, &user_config)?;
              out.push('\n');
              out.push_str(&self.display_table(table)?);
              Ok(out)
            })
            .collect()
        }
      });

      use std::fmt::Write;
      let mut out = String::new();

      let repo_result = repo_thead.join().unwrap();
      match repo_result {
        Ok(branches) => writeln!(out, "{}", branches)?,
        Err(e) => writeln!(out, "{}", e)?,
      }

      let proj_results = proj_thread.join().unwrap();
      for result in proj_results {
        match result {
          Ok(branches) => writeln!(out, "{}", branches)?,
          Err(e) => writeln!(out, "{}", e)?,
        }
      }

      let mod_results = mod_thread.join().unwrap();
      for result in mod_results {
        match result {
          Ok(branches) => writeln!(out, "{}", branches)?,
          Err(e) => writeln!(out, "{}", e)?,
        }
      }

      Ok(out)
    })?;

    if is_term() {
      let trunc = get_term_width();
      for line in out.lines() {
        let text = truncate_str(line, trunc, &style("\u{2026}").dim().to_string());
        println!("{}", text);
      }
    } else {
      println!("{}", out);
    }

    Ok(())
  }

  fn build_row(&self, repo: &Repository, user_config: &UserConfig, branch: &Branch) -> Result<Row> {
    let mut row = Row::default();

    let branch_name = branch.name_bytes()?.to_str_lossy();
    let current = get_current_branch_name(repo)?;
    let wt_branches = get_worktree_branch_names(repo)?;

    row.branch = if current.is_some_and(|current| current == branch_name) {
      // highlight checked-out branch green
      style(&branch_name).bold().green().to_string()
    } else if wt_branches
      .iter()
      .any(|wt_branch| wt_branch == &branch_name)
    {
      // highlight checked-out worktree branches cyan
      style(&branch_name).bold().cyan().to_string()
    } else {
      style(&branch_name).bold().to_string()
    };

    let branch_commit = branch.get().peel_to_commit()?;

    row.commit = style!(
      "{}, {} · {}",
      branch_commit.author().name()?,
      display_time(&branch_commit.time(), &DisplayTimeOptions {
        relative: user_config.format_relative()?,
        fmt: user_config.format_date()?,
      })?,
      branch_commit
        .summary()?
        .expect("Commit should have a summary")
    )
    .dim()
    .to_string();

    if let Some(upstream) = branch.upstream().not_found_ok()? {
      let upstream_name = upstream.name_bytes()?.to_str_lossy();

      let (a, b) = repo
        .graph_ahead_behind(upstream.get().peel_to_commit()?.id(), branch_commit.id())
        .with_context(|| {
          format!(
            "Failed to get ahead/behind between {} and {}",
            &branch_name, &upstream_name
          )
        })?;

      row.upstream = Some(display_plus_minus(a, b));
    }

    let base = user_config.branch_base(&branch_name)?;
    if let Some(base) = base {
      let (a, b) = repo
        .graph_ahead_behind(
          base.resolve(repo)?.peel_to_commit()?.id(),
          branch_commit.id(),
        )
        .with_context(|| {
          format!(
            "Failed to get ahead/behind between {} and {}",
            &branch_name,
            base.name()
          )
        })?;

      row.base = Some(display_plus_minus(a, b));
    }

    Ok(row)
  }

  fn build_table(&self, repo: &Repository, user_config: &UserConfig) -> Result<Vec<Row>> {
    let mut table = Vec::new();

    let branches = repo.branches(Some(BranchType::Local))?;
    for (branch, _) in branches.flatten() {
      let row = self.build_row(repo, user_config, &branch)?;
      table.push(row);
    }

    Ok(table)
  }

  /// Calculates the width of the left column, which includes the name and
  /// ahead/behind counts
  fn calculate_column_width(&self, table: &Vec<Row>) -> usize {
    let mut width = 0usize;

    for row in table {
      let mut w = measure_text_width(&row.branch);

      // ahead/behind:
      // branch-name U +15 -10 B +18 -0
      //
      // - the ab string in each contains just the numbers, the +/- char, and a space
      //   in the middle
      // - need to add a the indicator char (U or B) and the space separating that
      // - need to add the leading space, either separates from the branch name or the
      //   upstream ahead/behind

      if let Some(ab) = row.upstream.as_deref() {
        w += measure_text_width(ab) + 3;
      }

      if let Some(ab) = row.base.as_deref() {
        w += measure_text_width(ab) + 3;
      }

      if w > width {
        width = w;
      }
    }

    width
  }

  fn display_table(&self, table: Vec<Row>) -> Result<String> {
    use std::fmt::Write;
    let mut out = String::new();

    let width = self.calculate_column_width(&table);

    let mut first = true;
    let mut ab_buf = String::with_capacity(20);

    for row in table {
      if first {
        first = false;
      } else {
        writeln!(out)?;
      }

      write!(out, "{}", &row.branch)?;

      if let Some(upstream) = &row.upstream {
        // include leading space
        write!(ab_buf, " {} {}", style("U").blue(), upstream)?;
      }

      if let Some(base) = &row.base {
        // include leading space here too
        write!(ab_buf, " {} {}", style("B").magenta(), base)?;
      }

      let name_width = measure_text_width(&row.branch);
      let ab_width = measure_text_width(&ab_buf);
      let padding = width - name_width - ab_width;

      write!(out, "{}{}", " ".repeat(padding), ab_buf)?;
      ab_buf.clear();

      write!(out, " {}", &row.commit)?;
    }

    Ok(out)
  }
}
