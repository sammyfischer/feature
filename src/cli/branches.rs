use std::thread;

use anyhow::{Context, Result};
use console::{measure_text_width, style, truncate_str};
use git2::{Branch, BranchType, Repository};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::cli::display::commit::display_commit_compact;
use crate::cli::display::display_plus_minus;
use crate::cli::display::time::{DisplayTimeOptions, display_time};
use crate::cli::term::{get_term_width, is_term};
use crate::core::branch::{get_current_branch_name, get_worktree_branch_names};
use crate::core::branch_info::BranchInfo;
use crate::core::string::{ToStrLossy, ToStrLossyOwned};
use crate::core::user_config::UserConfig;
use crate::core::{NotFoundExt, open_repo_from_dirs, trim_hash};
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

  /// Glob pattern to filter branch names
  #[arg(value_name = "GLOB")]
  pattern: Option<String>,
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

        self.display_branches(&repo, &user_config, self.pattern.as_deref())
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

              out.push('\n');
              out.push_str(&self.display_branches(&repo, &user_config, self.pattern.as_deref())?);
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
              let repo = module.open()?;

              let mut out = format!("\n{} {}", style("Module").bold(), style(name).cyan());
              out.push('\n');
              out.push_str(&self.display_branches(&repo, &user_config, self.pattern.as_deref())?);
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

  /// Gets the list of branches and decides whether to display in single or list
  /// mode. Returns the entire formatted output.
  fn display_branches(
    &self,
    repo: &Repository,
    user_config: &UserConfig,
    glob: Option<&str>,
  ) -> Result<String> {
    let branches: Vec<Branch> = if let Some(glob) = glob {
      repo
        .references_glob(&format!("refs/heads/{}", glob))?
        .flatten()
        .filter_map(|rf| {
          // with the above glob pattern this should always be true, but it's safe to
          // double check
          if rf.is_branch() {
            Some(Branch::wrap(rf))
          } else {
            None
          }
        })
        .collect()
    } else {
      repo
        .branches(Some(BranchType::Local))?
        .flatten()
        .map(|(branch, _)| branch)
        .collect()
    };

    if branches.len() == 1 {
      self.display_single_branch(repo, user_config, &branches[0])
    } else {
      let mut table = Vec::with_capacity(branches.len());
      for branch in branches {
        let row = self.build_row(repo, user_config, &branch)?;
        table.push(row);
      }

      self.display_table(table, user_config)
    }
  }

  /// Create a row in the table from a branch
  fn build_row(&self, repo: &Repository, user_config: &UserConfig, branch: &Branch) -> Result<Row> {
    let mut row = Row::default();

    let branch_name = branch.name_bytes()?.to_str_lossy();
    let current = get_current_branch_name(repo)?;
    let wt_branches = get_worktree_branch_names(repo)?;

    let trunc_name = truncate_str(&branch_name, 30, "\u{2026}");

    row.branch = if current.is_some_and(|current| current == branch_name) {
      // highlight checked-out branch green
      style(&trunc_name).bold().green().to_string()
    } else if wt_branches
      .iter()
      .any(|wt_branch| wt_branch == &branch_name)
    {
      // highlight checked-out worktree branches cyan
      style(&trunc_name).bold().cyan().to_string()
    } else {
      style(&trunc_name).bold().to_string()
    };

    let branch_commit = branch.get().peel_to_commit()?;
    row.commit = display_commit_compact(&branch_commit, user_config, false)?;

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

  /// Calculates the width of the left column of branch list, which includes the
  /// name and ahead/behind counts. The width is used to align the commit text.
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

  /// Displays a list of branches in a somewhat tabular format
  fn display_table(&self, table: Vec<Row>, user_config: &UserConfig) -> Result<String> {
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

      let nerdfont = user_config.nerdfont()?;

      if let Some(upstream) = &row.upstream {
        // include leading space
        write!(
          ab_buf,
          " {} {}",
          style(if nerdfont { "" } else { "U" }).blue(),
          upstream
        )?;
      }

      if let Some(base) = &row.base {
        // include leading space here too
        write!(
          ab_buf,
          " {} {}",
          style(if nerdfont { "" } else { "B" }).magenta(),
          base
        )?;
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

  /// If a single branch is matched, display in high detail
  fn display_single_branch(
    &self,
    repo: &Repository,
    user_config: &UserConfig,
    branch: &Branch,
  ) -> Result<String> {
    use std::fmt::Write;
    let mut out = String::with_capacity(100);

    let info = BranchInfo::from_branch(branch)?;

    // highlight name the same way as list mode
    let current = get_current_branch_name(repo)?;
    let wt_branches = get_worktree_branch_names(repo)?;

    if current.is_some_and(|it| it == info.name()) {
      write!(out, "{}", style(info.name()).green())?;
    } else if wt_branches.iter().any(|it| it == info.name()) {
      write!(out, "{}", style(info.name()).cyan())?;
    } else {
      write!(out, "{}", info.name())?;
    }

    // branch-name
    //
    //  355daf4 Author Name, 15 hours ago
    //  fix(stash): add status output after stash pop
    //
    //  Base     origin/branch-status +0 -4
    //  Upstream origin/main          +0 -0
    //
    // no nerd font:
    // branch-name
    //
    // 355daf4 Author Name, 15 hours ago
    // fix(stash): add status output after stash pop
    //
    // Base     origin/branch-status +0 -4
    // Upstream origin/main          +0 -0

    let commit = branch.get().peel_to_commit()?;

    let nerdfont = user_config.nerdfont()?;
    write!(
      out,
      "\n\n{}",
      style!(
        "{}{}",
        if nerdfont { " " } else { "" },
        trim_hash(commit.as_object())?
      )
      .yellow()
    )?;

    write!(
      out,
      " {}, {}",
      commit.author().name()?,
      display_time(&commit.time(), &DisplayTimeOptions {
        relative: user_config.format_relative()?,
        fmt: user_config.format_date()?
      })?
    )?;

    write!(
      out,
      "\n{}",
      style!(
        "{}{}",
        if nerdfont { " " } else { "" },
        commit.summary()?.expect("Commit should have a summary")
      )
      .dim()
    )?;

    /// Branch ahead/behind is printed as a table:
    ///  Upstream origin/main          +0 -0
    ///  Base     origin/branch-status +0 -4
    ///
    /// This represents a row in that table
    struct BranchRow {
      /// Upstream or base, possibly with the icon
      label: String,

      /// Branch name
      name: String,

      /// Ahead/behind this branch (upstream/base) vs. the branch being listed
      ab: (usize, usize),
    }

    let mut branch_rows = Vec::with_capacity(2);

    if let Some(upstream) = branch.upstream().not_found_ok()? {
      let upstream_tip = upstream.get().peel_to_commit()?.id();
      let ab = repo.graph_ahead_behind(upstream_tip, commit.id())?;

      let upstream_row = BranchRow {
        label: style!("{}{}", if nerdfont { " " } else { "" }, "Upstream")
          .blue()
          .to_string(),
        name: upstream
          .name()?
          .expect("Upstream should have a name")
          .to_string(),
        ab,
      };

      branch_rows.push(upstream_row);
    }

    if let Some(base) = user_config.branch_base(info.name())? {
      let base_tip = base.resolve(repo)?.peel_to_commit()?.id();
      let ab = repo.graph_ahead_behind(base_tip, commit.id())?;

      let base_row = BranchRow {
        label: style!("{}{}", if nerdfont { " " } else { "" }, "Base")
          .magenta()
          .to_string(),
        name: base.name().to_string(),
        ab,
      };

      branch_rows.push(base_row);
    }

    let mut label_width = 0usize;
    let mut name_width = 0usize;

    for row in &branch_rows {
      label_width = label_width.max(measure_text_width(&row.label));
      name_width = name_width.max(measure_text_width(&row.name));
    }

    if !branch_rows.is_empty() {
      // double space
      writeln!(out)?;
    }

    for row in &branch_rows {
      let label = {
        let padding = label_width - measure_text_width(&row.label);
        format!("{}{}", row.label, " ".repeat(padding))
      };

      let name = {
        let padding = name_width - measure_text_width(&row.name);
        format!("{}{}", row.name, " ".repeat(padding))
      };

      let (a, b) = row.ab;
      write!(out, "\n{} {} {}", label, name, display_plus_minus(a, b))?;
    }

    Ok(out)
  }
}
