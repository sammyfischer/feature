use std::thread;

use anyhow::{Result, anyhow};
use console::{measure_text_width, style, truncate_str};
use git2::{ErrorClass, ErrorCode, Repository};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::cli::display::commit::display_commit_compact;
use crate::cli::display::diff::display_summary_header;
use crate::cli::display::time::{DisplayTimeOptions, display_time};
use crate::cli::term::{get_term_width, is_term};
use crate::core::diff::DiffSummary;
use crate::core::string::ToStrLossyOwned;
use crate::core::tag::{SemverTag, find_current_semver, get_semver_tags};
use crate::core::user_config::UserConfig;
use crate::core::{NotFoundExt, open_repo_from_dirs};
use crate::{App, if_nerdfont, style};

const LONG_ABOUT: &str = r#"Lists semver tags, sorted by version (highest to lowest). Shows how many commits
were added since the previous version. If the tag is annotated, it shows the tag
author and message. If it's lightweight, it shows the author/message of the
commit it points to."#;

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Lists semver tags", long_about = LONG_ABOUT, disable_help_subcommand = true)]
pub struct Args {
  /// Hides feature projects from output
  #[arg(short = 'P', long, value_name = "HIDE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  no_projects: Option<bool>,

  /// Hides git submodules from output
  #[arg(short = 'M', long, value_name = "HIDE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  no_modules: Option<bool>,

  /// View detailed info about a particular tag
  #[arg(value_name = "TAG")]
  tag: Option<String>,
}

#[derive(Debug, Default)]
struct Row {
  /// The name of the tag
  tag: String,

  /// The tagger for annotated tags, or the commit author for lightweight tags
  author: String,

  /// The time the tag obj was created (annotated), or the time the commit was
  /// created (lightweight)
  time: String,

  /// The tag obj message (annotated), or the commit message (lightweight)
  msg: String,

  /// Number of commits since prev version (stringified)
  since_prev: Option<String>,
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

        match &self.tag {
          Some(name) => self.display_single_tag(&repo, name),
          None => {
            let table = self.build_table(&repo)?;
            self.display_table(table)
          }
        }
      });

      let proj_thread = scope.spawn(|| {
        if hide_projects {
          Vec::new()
        } else {
          proj_config
            .projects
            .par_iter()
            .map(|(name, project)| -> Result<_> {
              let mut out = format!("\n{} {}\n", style("Project").bold(), style(name).cyan());
              let repo = Repository::open(&project.path)?;

              match &self.tag {
                Some(name) => self.display_single_tag(&repo, name),
                None => {
                  let table = self.build_table(&repo)?;
                  out.push_str(&self.display_table(table)?);
                  Ok(out)
                }
              }
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
              let repo = module.open()?;
              let mut out = format!("\n{} {}\n", style("Module").bold(), style(name).cyan());

              match &self.tag {
                Some(name) => self.display_single_tag(&repo, name),
                None => {
                  let table = self.build_table(&repo)?;
                  out.push_str(&self.display_table(table)?);
                  Ok(out)
                }
              }
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

  fn build_row(&self, repo: &Repository, config: &UserConfig, tag: &SemverTag) -> Result<Row> {
    let mut row = Row::default();

    let name = tag.name();
    row.tag = style(&name).bold().to_string();

    let commit = repo.find_commit(tag.commit)?;
    let reference = repo.resolve_reference_from_short_name(&name)?;

    if let Some(parent) = commit.parent(0).not_found_ok()?
      && let Some(prev) = find_current_semver(repo, &parent)?
    {
      let (distance, _) = repo.graph_ahead_behind(commit.id(), prev.commit)?;
      row.since_prev = Some(distance.to_string());
    }

    let time_opts = DisplayTimeOptions {
      relative: config.format_relative()?,
      fmt: config.format_date()?,
    };

    match reference.peel_to_tag() {
      // annotated tag
      Ok(obj) => {
        match obj.tagger() {
          Some(sig) => {
            row.author = sig.name()?.to_string();
            row.time = display_time(&sig.when(), &time_opts)?;
          }
          None => {
            row.author = commit.author().name()?.to_string();
            row.time = display_time(&commit.time(), &time_opts)?;
          }
        };

        row.msg = match obj.message()? {
          Some(msg) => msg.to_string(),
          None => commit.summary()?.unwrap_or(commit.message()?).to_string(),
        };
      }

      // lightweight tag
      Err(e) if e.class() == ErrorClass::Object && e.code() == ErrorCode::InvalidSpec => {
        row.author = commit.author().name()?.to_string();
        row.time = display_time(&commit.time(), &time_opts)?;
        row.msg = commit.summary()?.unwrap_or(commit.message()?).to_string();
      }

      Err(e) => return Err(anyhow!(e)),
    }

    Ok(row)
  }

  fn build_table(&self, repo: &Repository) -> Result<Vec<Row>> {
    let repo_dir = repo.path();
    let work_dir = repo.workdir();

    let mut tags = get_semver_tags(repo)?;
    tags.sort_by(|a, b| b.cmp(a));

    // each tag walks the commit graph to find the previous tag, which can be slow
    let table = tags
      .par_iter()
      .map(|tag| {
        let repo = open_repo_from_dirs(repo_dir, work_dir)?;
        let config = UserConfig::new(&repo)?;
        self.build_row(&repo, &config, tag)
      })
      .collect::<Result<Vec<_>>>()?;

    Ok(table)
  }

  /// Width of the left column containing tag name and number of commits
  fn calculate_column_width(&self, table: &Vec<Row>) -> usize {
    let mut width = 0usize;

    for row in table {
      let mut w = measure_text_width(&row.tag);

      if let Some(d) = &row.since_prev {
        // add one for the space, and another for the +
        w += measure_text_width(d) + 2;
      }

      width = width.max(w);
    }

    width
  }

  fn display_table(&self, table: Vec<Row>) -> Result<String> {
    use std::fmt::Write;
    let mut out = String::new();

    let width = self.calculate_column_width(&table);

    let mut first = true;
    let mut distance_buf = String::with_capacity(4);
    for row in table {
      if first {
        first = false;
      } else {
        writeln!(out)?;
      }

      write!(out, "{}", row.tag)?;

      if let Some(d) = row.since_prev {
        write!(distance_buf, " {}", style!("+{}", d).green())?;
      }

      let padding = width - measure_text_width(&row.tag) - measure_text_width(&distance_buf);
      write!(out, "{}{}", " ".repeat(padding), &distance_buf)?;
      distance_buf.clear();

      write!(
        out,
        " {}",
        style!("{}, {} · {}", row.author, row.time, row.msg).dim()
      )?;
    }

    Ok(out)
  }

  fn display_single_tag(&self, repo: &Repository, name: &str) -> Result<String> {
    use std::fmt::Write;
    let mut out = String::new();

    let config = UserConfig::new(repo)?;
    let rf = repo.resolve_reference_from_short_name(name)?;

    if !rf.is_tag() {
      return Err(anyhow!(format!("{} is not a tag", name)));
    }

    write!(out, "{}", style(name).green())?;

    let nerdfont = config.nerdfont()?;
    let time_opts = DisplayTimeOptions {
      relative: config.format_relative()?,
      fmt: config.format_date()?,
    };

    match rf.peel_to_tag() {
      // annotated tag
      Ok(obj) => {
        if let Some(sig) = obj.tagger() {
          write!(
            out,
            "\n\n{}{}, {}",
            style(if_nerdfont!(nerdfont, " ")).yellow(),
            sig.name()?,
            display_time(&sig.when(), &time_opts)?
          )?;

          if let Some(msg) = obj.message()? {
            write!(
              out,
              "\n{}",
              style!("{}{}", if_nerdfont!(nerdfont, " "), msg).dim()
            )?
          };
        };
      }

      // lightweight tag
      Err(e) if e.class() == ErrorClass::Object && e.code() == ErrorCode::InvalidSpec => {}

      Err(e) => return Err(anyhow!(e)),
    }

    let commit = rf.peel_to_commit()?;
    write!(
      out,
      "\n\n{}",
      display_commit_compact(&commit, &config, true)?
    )?;

    write!(
      out,
      "\n{}",
      style!(
        "{}{}",
        if_nerdfont!(nerdfont, " "),
        commit.summary()?.unwrap_or(commit.message()?)
      )
      .dim()
    )?;

    let old_tree = if let Some(parent) = commit.parent(0).not_found_ok()?
      && let Some(prev) = find_current_semver(repo, &parent)?
    {
      write!(out, "\n\nSince {}", style(prev.name()).yellow())?;

      let (ahead, _) = repo.graph_ahead_behind(commit.id(), prev.commit)?;
      write!(
        out,
        " - {} {}, ",
        style(ahead).cyan(),
        if ahead == 1 { "commit" } else { "commits" }
      )?;

      Some(repo.find_commit(prev.commit)?.tree()?)
    } else {
      write!(out, "\n\nInitial release - ")?;
      None
    };

    let mut diff = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&commit.tree()?), None)?;
    diff.find_similar(None)?;
    let summary = DiffSummary::new(&diff)?;

    write!(out, "{}", display_summary_header(&summary))?;

    Ok(out)
  }
}
