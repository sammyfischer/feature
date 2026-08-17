use std::cmp::Reverse;
use std::thread;

use anyhow::{Context, Result};
use console::{measure_text_width, style, truncate_str};
use git2::{Branch, BranchType, Repository, Time};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::cli::SortBy;
use crate::cli::display::display_plus_minus;
use crate::cli::display::time::{DisplayTimeOptions, display_time};
use crate::cli::term::{get_term_width, is_term};
use crate::core::NotFoundExt;
use crate::core::branch::{get_current_branch_name, get_worktree_branch_names};
use crate::core::string::{ToStrLossy, ToStrLossyOwned};
use crate::core::threading::ThreadedRepoHandle;
use crate::core::user_config::UserConfig;
use crate::core::wip::WipList;
use crate::{App, style};

const LONG_ABOUT: &str = r#"Lists branches.

By default, it will show all local branches. Specify a glob pattern to filter
down the list.

Legend:
• Green branch name - currently checked out
• Cyan branch name - checked out in a worktree
• Yellow icon after name - wips exist on the branch
• Magenta icon with counts - ahead/behind count for upstream branch
• Blue icon with counts - ahead/behind count for base branch

The actual icon displayed depends on the "feature.nerdfont" config option.

If the list gets filtered down to a single branch, it will display high-detail
info about that particular branch instead of using the default compact format."#;

#[derive(clap::Args, Clone, Debug)]
#[command(
  visible_alias = "branches",
  about = "Lists branches",
  long_about = LONG_ABOUT,
  disable_help_subcommand = true
)]
pub struct BranchListArgs {
  /// How to sort the list
  #[arg(short, long)]
  sort: Option<SortBy>,

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

impl BranchListArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let handle = ThreadedRepoHandle::from(&state.repo);

    let proj_config = &state.config;
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
        let repo = handle.open()?;
        let user_config = UserConfig::new(&repo)?;

        self.display_branches(&repo, &user_config, self.pattern.as_deref())
      });

      let proj_thread = scope.spawn(|| {
        if hide_projects {
          Vec::new()
        } else {
          proj_config
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
              let repo = handle.open()?;
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
    /// A branch with its sort keys
    struct SortableBranch<'branch> {
      branch: Branch<'branch>,
      time: Time,
      name: String,
    }

    let mut branches: Vec<SortableBranch> = if let Some(glob) = glob {
      let refs = repo
        .references_glob(&format!("refs/heads/{}", glob))?
        .flatten();

      let mut branches = Vec::new();
      for rf in refs {
        let commit = rf.peel_to_commit()?;
        let branch = Branch::wrap(rf);
        let name = branch
          .name()?
          .expect("Branch names should be valid utf-8")
          .to_string();

        branches.push(SortableBranch {
          branch,
          name,
          time: commit.time(),
        });
      }

      branches
    } else {
      let branch_iter = repo
        .branches(Some(BranchType::Local))?
        .flatten()
        .map(|(branch, _)| branch);

      let mut branches = Vec::new();
      for branch in branch_iter {
        let commit = branch.get().peel_to_commit()?;
        let name = branch
          .name()?
          .expect("Branch names should be valid utf-8")
          .to_string();

        branches.push(SortableBranch {
          branch,
          name,
          time: commit.time(),
        });
      }

      branches
    };

    match self.sort.unwrap_or_default() {
      SortBy::Date => branches.sort_by_key(|it| Reverse(it.time)),

      // can't use sort_by_key bc of weird borrowing reasons
      SortBy::Name => branches.sort_by(|a, b| a.name.cmp(&b.name)),
    }

    let branches: Vec<Branch> = branches.into_iter().map(|it| it.branch).collect();

    let mut table = Vec::with_capacity(branches.len());
    for branch in branches {
      let row = self.build_row(repo, user_config, &branch)?;
      table.push(row);
    }

    self.display_table(table, user_config)
  }

  /// Create a row in the table from a branch
  fn build_row(&self, repo: &Repository, user_config: &UserConfig, branch: &Branch) -> Result<Row> {
    let mut row = Row::default();

    let branch_name = branch.name_bytes()?.to_str_lossy();
    let current = get_current_branch_name(repo)?;
    let wt_branches = get_worktree_branch_names(repo)?;

    let trunc_name = truncate_str(&branch_name, 20, "\u{2026}");

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

    // wip indicator. uses the same icon as dirty workdir indicator in status,
    // since wips are meant to be thought of as uncommitted changes that exist
    // on a branch
    let wips = WipList::from_branch(repo, branch_name.to_string())?;
    if !wips.is_empty() {
      row.branch.push_str(&format!(" {}", style("●").yellow()));
    }

    let branch_commit = branch.get().peel_to_commit()?;
    row.commit = format!(
      "{}",
      style!(
        "{} · {}",
        display_time(
          &branch_commit.time(),
          &DisplayTimeOptions::try_from(user_config)?
        )?,
        branch_commit.summary()?.unwrap_or(branch_commit.message()?)
      )
      .dim()
    );

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

    if let Some(base) = user_config.branch_base(&branch_name)? {
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
}
