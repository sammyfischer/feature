use std::thread;

use anyhow::{Context, Result};
use console::style;
use git2::{Branch, BranchType, Repository};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::App;
use crate::cli::display::time::{DisplayTimeOptions, display_time};
use crate::cli::display::{display_hash, display_plus_minus};
use crate::cli::term::paginate;
use crate::core::branch::{get_current_branch_name, get_worktree_branch_names};
use crate::core::string::{ToStrLossy, ToStrLossyOwned};
use crate::core::user_config::UserConfig;
use crate::core::{NotFoundExt, open_repo_from_dirs};

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

    paginate(out.as_bytes())?;
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

    // hash Author Name, 1 hour ago - Subject line
    // hash is yellow, so it doesn't need a character to separate it from author
    // name. everything else is gray
    row.commit = format!(
      "{} {}",
      display_hash(branch_commit.as_object())?,
      style(&format!(
        "{}, {} · {}",
        branch_commit.author().name_bytes().to_str_lossy(),
        display_time(&branch_commit.time(), &DisplayTimeOptions {
          relative: true,
          // fmt is irrelevant for relative times. `String::new` doesn't
          // allocate so this is fine
          fmt: String::new(),
        })?,
        branch_commit
          .summary_bytes()
          .expect("Commit should have a summary")
          .to_str_lossy()
      ))
      .dim(),
    );

    if let Some(upstream) = branch.upstream().not_found_ok()? {
      let upstream_name = upstream.name_bytes()?.to_str_lossy();
      let mut col = style(&upstream_name).blue().to_string();

      let (a, b) = repo
        .graph_ahead_behind(upstream.get().peel_to_commit()?.id(), branch_commit.id())
        .with_context(|| {
          format!(
            "Failed to get ahead/behind between {} and {}",
            &branch_name, &upstream_name
          )
        })?;

      col.push(' ');
      col.push_str(&display_plus_minus(a, b));
      row.upstream = Some(col);
    }

    let base = user_config.branch_base(&branch_name)?;
    if let Some(base) = base {
      let mut col = style(base.name()).magenta().to_string();

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

      col.push(' ');
      col.push_str(&display_plus_minus(a, b));
      row.base = Some(col);
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

  fn display_table(&self, table: Vec<Row>) -> Result<String> {
    use std::fmt::Write;
    let mut out = String::new();

    // we want every base/upstream name to be aligned
    let mut label_width = 0usize;
    const UPSTREAM_LABEL: usize = "upstream".len();
    const BASE_LABEL: usize = "base".len();

    for row in &table {
      if row.upstream.is_some() && UPSTREAM_LABEL > label_width {
        label_width = UPSTREAM_LABEL;
      }
      if row.base.is_some() && BASE_LABEL > label_width {
        label_width = BASE_LABEL;
      }
    }

    let mut first = true;
    for row in table {
      if first {
        first = false;
      } else {
        writeln!(out)?;
      }

      write!(out, "{} {}", row.branch, row.commit)?;
      if let Some(upstream) = row.upstream {
        write!(
          out,
          "\n  {}{} {}{}{}",
          style("upstream").blue(),
          " ".repeat(label_width - UPSTREAM_LABEL),
          style("[").dim(),
          upstream,
          style("]").dim()
        )?;
      }
      if let Some(base) = row.base {
        write!(
          out,
          "\n  {}{} {}{}{}",
          style("base").magenta(),
          " ".repeat(label_width - BASE_LABEL),
          style("[").dim(),
          base,
          style("]").dim()
        )?;
      }
    }

    Ok(out)
  }
}
