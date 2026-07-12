use std::thread;

use anyhow::{Result, anyhow};
use console::style;
use git2::{ErrorClass, ErrorCode, Repository};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::cli::display::display_hash;
use crate::cli::display::time::{DisplayTimeOptions, display_time, display_time_relative};
use crate::cli::term::paginate;
use crate::core::open_repo_from_dirs;
use crate::core::string::{ToStrLossy, ToStrLossyOwned};
use crate::core::tag::{SemverTag, get_semver_tags};
use crate::core::user_config::UserConfig;
use crate::{App, style};

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Lists semver tags", disable_help_subcommand = true)]
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
  tag: String,
  commit: String,
  tagger: Option<String>,
}

impl Args {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo_dir = state.repo.path().to_owned();
    let work_dir = state.repo.workdir().to_owned();
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
        let repo = open_repo_from_dirs(&repo_dir, work_dir)?;

        let table = self.build_table(&repo)?;
        self.display_table(table)
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

              let table = self.build_table(&repo)?;
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
              let module = repo.find_submodule(name)?;
              let mod_repo = module.open()?;

              let mut out = format!("\n{} {}", style("Module").bold(), style(name).cyan());
              let table = self.build_table(&mod_repo)?;
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

  fn build_row(&self, repo: &Repository, tag: &SemverTag) -> Result<Row> {
    let mut row = Row::default();

    let name = tag.name();
    row.tag = style(&name).bold().to_string();

    let commit = repo.find_commit(tag.commit)?;

    row.commit = format!(
      "{} {}",
      display_hash(commit.as_object())?,
      style(&format!(
        "{}, {} · {}",
        commit.author().name_bytes().to_str_lossy(),
        display_time(&commit.time(), &DisplayTimeOptions {
          relative: true,
          // fmt is irrelevant for relative times. `String::new` doesn't
          // allocate so this is fine
          fmt: String::new(),
        })?,
        commit
          .summary_bytes()
          .expect("Commit should have a summary")
          .to_str_lossy()
      ))
      .dim(),
    );

    let reference = repo.resolve_reference_from_short_name(&name)?;

    match reference.peel_to_tag() {
      Ok(obj) => {
        row.tagger = match (obj.tagger(), obj.message_bytes()) {
          (None, None) => None,

          (Some(sig), Some(msg)) => Some(
            style!(
              "{}, {} · {}",
              sig.name_bytes().to_str_lossy(),
              display_time_relative(&sig.when())?,
              msg.to_str_lossy()
            )
            .dim()
            .to_string(),
          ),

          (Some(sig), None) => Some(
            style!(
              "{}, {}",
              sig.name_bytes().to_str_lossy(),
              display_time_relative(&sig.when())?
            )
            .dim()
            .to_string(),
          ),

          (None, Some(msg)) => Some(style(msg.to_str_lossy()).dim().to_string()),
        };
      }

      Err(e) if e.class() == ErrorClass::Object && e.code() == ErrorCode::InvalidSpec => {}
      Err(e) => return Err(anyhow!(e)),
    }

    Ok(row)
  }

  fn build_table(&self, repo: &Repository) -> Result<Vec<Row>> {
    let mut table = Vec::new();

    let mut tags = get_semver_tags(repo)?;
    tags.sort_by(|a, b| b.cmp(a));

    for tag in &tags {
      table.push(self.build_row(repo, tag)?);
    }

    Ok(table)
  }

  fn display_table(&self, table: Vec<Row>) -> Result<String> {
    use std::fmt::Write;
    let mut out = String::new();

    let mut first = true;
    for row in table {
      if first {
        first = false;
      } else {
        writeln!(out)?;
      }

      write!(out, "{} {}", row.tag, row.commit)?;
      if let Some(tagger) = row.tagger {
        write!(out, "\n  {}", tagger)?;
      }
    }

    Ok(out)
  }
}
