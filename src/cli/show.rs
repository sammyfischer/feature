use anyhow::{Context, Result, anyhow};
use clap::ValueHint;
use console::{measure_text_width, style, truncate_str};
use git2::{Branch, Commit, Repository, Tag};

use crate::cli::display::commit::{DisplayCommitOptions, display_commit};
use crate::cli::display::diff::{display_summary, display_summary_header};
use crate::cli::display::display_plus_minus;
use crate::cli::display::time::{DisplayTimeOptions, display_time};
use crate::cli::term::{is_term, paginate};
use crate::cli::version::display_tag;
use crate::cli::wip::display_wip;
use crate::core::branch::{get_current_branch_name, get_head_resolved, get_worktree_branch_names};
use crate::core::branch_info::BranchInfo;
use crate::core::commit::get_commit_decorations;
use crate::core::diff::{DiffSummary, get_formatted_diff};
use crate::core::project_config::{PageWhen, ProjectConfig};
use crate::core::string::{ToStrLossy, TrimPrefix};
use crate::core::user_config::{CommitMessageLevel, UserConfig};
use crate::core::version::{VersionTag, find_current_version, since_prev_version};
use crate::core::wip::WipList;
use crate::core::{NotFoundExt, trim_hash};
use crate::{App, if_nerdfont, style};

const LONG_ABOUT: &str = r#"Show info about a commit

The revision string will be used to determine the most useful display mode. If
the determined mode isn't what you want, you can specify a display mode with
"--display <mode>".

Some important details for each mode:
• branch mode is specifically for local branches, not remotes
• version mode only displays version tags
  • it also never displays diff patches
• tag mode is for displaying annotated tags, i.e. tag objects and the commit
  they point to
• commit mode is the fallback

For the options "--no-summary", and "--no-patch", an equals sign must be used
to specify a value. If no value is specified, "true" is assumed.

For example:
• "-S=false" to force the summary to appear
• "-S" to force the summary to be hidden"#;

#[derive(clap::Args, Debug)]
#[command(
  about = "Show info about a commit",
  long_about = LONG_ABOUT,
  disable_help_subcommand = true
)]
pub struct ShowArgs {
  /// Hide the diff summary
  #[arg(short = 'S', long, value_name = "HIDE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  no_summary: Option<bool>,

  /// Hide the diff patch
  #[arg(short = 'P', long, value_name = "HIDE", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
  no_patch: Option<bool>,

  /// How much of the commit message to show
  #[arg(short, long, value_name = "LEVEL")]
  message: Option<CommitMessageLevel>,

  /// When to page output
  #[arg(long, value_name = "WHEN")]
  paging: Option<PageWhen>,

  /// How to display the output
  #[arg(short, long, default_value = "auto", value_name = "MODE")]
  display: DisplayMode,

  /// Whether to display the object as a tag instead of a commit. This is only
  /// valid for revspecs that resolve to tags.
  #[arg(short, long)]
  tag: bool,

  /// The git revision string, e.g. HEAD^2, commit hash, branch name. See "man
  /// gitrevisions".
  #[arg(value_name = "REVISION", value_hint = ValueHint::Other)]
  revision: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
enum DisplayMode {
  /// Display details about a local branch
  Branch,

  /// Display details about a version tag
  Version,

  /// Display as tag object (annotated tag)
  Tag,

  /// Display as commit
  Commit,

  /// Resolve display mode based on revision string
  Auto,
}

/// The [DisplayMode], but with [Auto] resolved to one of the other modes.
///
/// [Auto]: DisplayMode::Auto
enum ResolvedDisplayMode<'data> {
  Branch(Branch<'data>),
  Version(VersionTag, Option<Tag<'data>>),
  Tag(Tag<'data>),
  Commit(Commit<'data>),
}

impl ShowArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let config = UserConfig::new(&state.repo)?;

    let rev = match self.revision.as_deref() {
      Some(rev) => rev.to_string(),
      None => get_head_resolved(&state.repo)?
        .context("No commits yet")?
        .name()?
        .to_string(),
    };

    let buf = match self.resolve_mode(state, &rev)? {
      ResolvedDisplayMode::Branch(branch) => self.show_branch(&state.repo, &config, &branch)?,

      ResolvedDisplayMode::Version(version, tag) => {
        self.show_version(&state.repo, &state.config, &version, tag.as_ref())?
      }

      ResolvedDisplayMode::Tag(tag) => self.show_tag(&state.repo, &config, &tag)?,

      ResolvedDisplayMode::Commit(commit) => self.show_commit(state, &config, &commit)?,
    };

    // use config value only if it's not explicitly set in the command line
    let paging = self.paging.unwrap_or(config.show_paging()?);
    match (paging, is_term()) {
      (PageWhen::Auto, true) | (PageWhen::Always, _) => paginate(&buf),
      (PageWhen::Auto, false) | (PageWhen::Never, _) => {
        print!("{}", buf.to_str_lossy());
        Ok(())
      }
    }
  }

  /// Determine the display mode based on the revision string and options
  ///
  /// # Lifetimes
  /// - `data` - the data contained in the [ResolvedDisplayMode] (e.g. branch,
  ///   commit, etc.). These all must be outlived by the repo contained in
  ///   `state`.
  fn resolve_mode<'data>(
    &self,
    state: &'data App,
    rev: &str,
  ) -> Result<ResolvedDisplayMode<'data>> {
    use DisplayMode as ModeIn;
    use ResolvedDisplayMode as ModeOut;

    let repo = &state.repo;
    let project_config = &state.config;

    let mode = match self.display {
      ModeIn::Branch => {
        let rf = repo.resolve_reference_from_short_name(rev)?;
        if !rf.is_branch() {
          return Err(anyhow!("Not a local branch: {}", rf.name()?));
        }

        ModeOut::Branch(Branch::wrap(rf))
      }

      ModeIn::Version => {
        let rf = repo.resolve_reference_from_short_name(rev)?;
        let name = rf.name()?;
        if !rf.is_tag() {
          return Err(anyhow!("Not a tag: {}", name));
        }

        let commit = rf
          .peel_to_commit()
          .with_context(|| format!("Failed to find commit pointed to by tag: {}", name))?
          .id();

        let ver = VersionTag::new(rev, commit);
        let tag = rf.peel_to_tag().tag_not_found_ok()?;

        ModeOut::Version(ver, tag)
      }

      ModeIn::Tag => {
        let rf = repo.resolve_reference_from_short_name(rev)?;
        let name = rf.name()?;
        if !rf.is_tag() {
          return Err(anyhow!("Not a tag: {}", name));
        }

        ModeOut::Tag(
          rf.peel_to_tag()
            .tag_not_found_ok()?
            .ok_or_else(|| anyhow!("Failed to resolve to annotated tag: {}", name))?,
        )
      }

      ModeIn::Commit => ModeOut::Commit(
        repo
          .revparse_single(rev)?
          .into_commit()
          .map_err(|_| anyhow!("Failed to resolve to commit: {}", rev))?,
      ),

      ModeIn::Auto => {
        let rf = repo.resolve_reference_from_short_name(rev).not_found_ok()?;

        if let Some(rf) = rf {
          if rf.is_branch() {
            return Ok(ModeOut::Branch(Branch::wrap(rf)));
          }

          if rf.is_tag() {
            let name = rf.shorthand()?;
            let tag = rf.peel_to_tag().tag_not_found_ok()?;

            let tag_names = repo.tag_names(Some(&project_config.version.pattern))?;

            for other in tag_names.iter().flatten() {
              if other.is_some_and(|other| name == other) {
                return Ok(ModeOut::Version(
                  VersionTag::new(name, rf.peel_to_commit()?.id()),
                  tag,
                ));
              }
            }

            // is annotated tag
            if let Some(tag) = tag {
              return Ok(ModeOut::Tag(tag));
            }
          }
        }

        // default to commit
        ModeOut::Commit(
          repo
            .revparse_single(rev)?
            .into_commit()
            .map_err(|_| anyhow!("Failed to resolve to commit: {}", rev))?,
        )
      }
    };

    Ok(mode)
  }

  fn show_branch(
    &self,
    repo: &Repository,
    config: &UserConfig,
    branch: &Branch,
  ) -> Result<Vec<u8>> {
    use std::io::Write;
    let mut buf: Vec<u8> = Vec::new();

    let info = BranchInfo::from_branch(branch)?;

    // highlight name the same way as list mode
    let current = get_current_branch_name(repo)?;
    let wt_branches = get_worktree_branch_names(repo)?;

    if current.is_some_and(|it| it == info.name()) {
      write!(buf, "{}", style(info.name()).green())?;
    } else if wt_branches.iter().any(|it| it == info.name()) {
      write!(buf, "{}", style(info.name()).cyan())?;
    } else {
      write!(buf, "{}", info.name())?;
    }

    // branch-name
    //
    //  355daf4 Author Name, 15 hours ago
    //  fix(stash): add status output after stash pop
    //
    //  Base     origin/branch-status +0 -4
    //  Upstream origin/main          +0 -0
    //
    // 󱉚 Wips
    // branch-name:0 3 weeks ago implement feature
    //
    // no nerd font:
    // branch-name
    //
    // 355daf4 Author Name, 15 hours ago
    // fix(stash): add status output after stash pop
    //
    // Base     origin/branch-status +0 -4
    // Upstream origin/main          +0 -0
    //
    // Wips
    // branch-name:0 3 weeks ago implement feature

    let commit = branch.get().peel_to_commit()?;

    let nerdfont = config.nerdfont()?;
    write!(
      buf,
      "\n\n{}",
      style!(
        "{}{}",
        if nerdfont { " " } else { "" },
        trim_hash(commit.as_object())?
      )
      .yellow()
    )?;

    write!(
      buf,
      " {}, {}",
      commit.author().name()?,
      display_time(&commit.time(), &DisplayTimeOptions::try_from(config)?)?
    )?;

    write!(
      buf,
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

    let base = config.branch_base(info.name())?;
    if let Some(base) = &base {
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
      writeln!(buf)?;
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
      write!(buf, "\n{} {} {}", label, name, display_plus_minus(a, b))?;
    }

    let wips = WipList::from_branch(repo, info.name().to_string())?;
    if !wips.is_empty() {
      write!(
        buf,
        "\n\n{}{}",
        style(if_nerdfont!(nerdfont, "󱉚 ")).cyan(),
        style("Wips").cyan()
      )?;

      for wip in wips.iter() {
        write!(
          buf,
          "\n{} {} {}",
          display_wip(&wip),
          style(display_time(
            &wip.time(),
            &DisplayTimeOptions::try_from(config)?
          )?)
          .magenta(),
          truncate_str(wip.message(), 72, &style("\u{2026}").dim().to_string())
        )?;
      }
    }

    if let Some(base) = &base {
      let base_tip = base.resolve(repo)?.peel_to_commit()?;
      let merge_base = repo.merge_base(commit.id(), base_tip.id())?;
      let (ahead, _) = repo.graph_ahead_behind(commit.id(), merge_base)?;

      let old_tree = repo.find_commit(merge_base)?.tree()?;

      let mut diff = repo.diff_tree_to_tree(Some(&old_tree), Some(&commit.tree()?), None)?;
      diff.find_similar(None)?;
      let summary = DiffSummary::new(&diff)?;

      let show_summary = match self.no_summary {
        Some(hide) => !hide,
        None => config.show_summary()?,
      };
      let show_patch = match self.no_patch {
        Some(hide) => !hide,
        None => config.show_patch()?,
      };

      if show_summary {
        write!(
          buf,
          "\n\nSince {} - {} {}, {}",
          style(base.name()).magenta(),
          style(ahead).cyan(),
          if ahead == 1 { "commit" } else { "commits" },
          display_summary(&summary, nerdfont)
        )?;
      }

      if show_patch {
        if !show_summary {
          // need to explain what this diff actually is, if the summary isn't shown
          write!(
            buf,
            "\n\nSince {} - {} {}",
            style(base.name()).magenta(),
            style(ahead).cyan(),
            if ahead == 1 { "commit" } else { "commits" }
          )?;
        }

        writeln!(buf)?;
        buf.extend_from_slice(&get_formatted_diff(&diff)?);
      }
    }

    Ok(buf)
  }

  fn show_version(
    &self,
    repo: &Repository,
    proj_config: &ProjectConfig,
    version: &VersionTag,
    tag: Option<&Tag>,
  ) -> Result<Vec<u8>> {
    use std::io::Write;
    let mut buf: Vec<u8> = Vec::new();

    let config = UserConfig::new(repo)?;

    let current = match get_head_resolved(repo)? {
      Some(head) => find_current_version(repo, proj_config, head.peel_to_commit()?.id())?
        .map(|(version, _)| version),
      None => None,
    };

    if current.is_some_and(|current| current.name() == version.name()) {
      write!(buf, "{}", style(version.name()).green())?;
    } else {
      write!(buf, "{}", version.name())?;
    }

    let nerdfont = config.nerdfont()?;
    let time_opts = DisplayTimeOptions::try_from(&config)?;

    if let Some(obj) = tag
      && let Some(sig) = obj.tagger()
    {
      write!(
        buf,
        "\n\n{}{}, {}",
        style(if_nerdfont!(nerdfont, " ")).yellow(),
        sig.name()?,
        display_time(&sig.when(), &time_opts)?
      )?;

      if let Some(msg) = obj.message()? {
        write!(
          buf,
          "\n{}",
          style!("{}{}", if_nerdfont!(nerdfont, " "), msg).dim()
        )?
      };
    };

    let commit = repo.find_commit(version.commit())?;
    write!(
      buf,
      "\n\n{}",
      style!(
        "{}{}",
        if nerdfont { " " } else { "" },
        trim_hash(commit.as_object())?
      )
      .yellow()
    )?;

    write!(
      buf,
      " {}, {}",
      commit.author().name()?,
      display_time(&commit.time(), &DisplayTimeOptions::try_from(&config)?)?
    )?;

    write!(
      buf,
      "\n{}",
      style!(
        "{}{}",
        if_nerdfont!(nerdfont, " "),
        commit.summary()?.unwrap_or(commit.message()?)
      )
      .dim()
    )?;

    let old_tree = if let Some((prev, _)) = since_prev_version(repo, proj_config, version)? {
      write!(buf, "\n\nSince {}", style(prev.name()).yellow())?;

      let (ahead, _) = repo.graph_ahead_behind(commit.id(), prev.commit())?;
      write!(
        buf,
        " - {} {}, ",
        style(ahead).cyan(),
        if ahead == 1 { "commit" } else { "commits" }
      )?;

      Some(repo.find_commit(prev.commit())?.tree()?)
    } else {
      write!(buf, "\n\nInitial release - ")?;
      None
    };

    let mut diff = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&commit.tree()?), None)?;
    diff.find_similar(None)?;
    let summary = DiffSummary::new(&diff)?;

    let show_summary = match self.no_summary {
      Some(hide) => !hide,
      None => config.show_summary()?,
    };

    if show_summary {
      write!(buf, "{}", display_summary(&summary, nerdfont))?;
    } else {
      write!(buf, "{}", display_summary_header(&summary))?;
    }

    Ok(buf)
  }

  fn show_commit(&self, state: &App, config: &UserConfig, commit: &Commit) -> Result<Vec<u8>> {
    use std::io::Write;
    let mut buf: Vec<u8> = Vec::new();

    // decorations like git log
    let decorations = get_commit_decorations(&state.repo, commit.id())?;
    if !decorations.is_empty() {
      let mut first = true;
      for rf in decorations {
        let name = rf.shorthand()?;

        let name = if name == "HEAD" {
          match rf.symbolic_target()? {
            Some(target) => style!(
              "HEAD -> {}",
              target
                .trim_prefix_opt("refs/heads/")
                .trim_prefix_opt("refs/remotes/")
            )
            .green()
            .to_string(),

            None => style("HEAD").green().to_string(),
          }
        } else {
          if rf.is_branch() {
            // local branch
            style(name).cyan()
          } else if rf.is_remote() {
            // upstream branch
            style(name).blue()
          } else if rf.is_tag() {
            // tag
            style(name).yellow()
          } else {
            // default
            style(name)
          }
          .to_string()
        };

        // print (comma separated)
        if first {
          first = false;
          write!(buf, "{}", name)?;
        } else {
          write!(buf, "{} {}", style(",").dim(), name)?;
        }
      }

      write!(buf, "\n\n")?;
    }

    writeln!(
      buf,
      "{}",
      display_commit(commit, &DisplayCommitOptions {
        message: self.message.unwrap_or(config.show_message()?),
        time: DisplayTimeOptions {
          relative: config.format_relative()?,
          fmt: config.format_date()?,
        }
      })?
    )?;

    let parent = commit.parent(0).not_found_ok()?;

    let new_tree = commit.tree()?;
    let old_tree = match parent {
      Some(it) => Some(it.tree()?),
      None => None,
    };

    let mut diff = state
      .repo
      .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)?;
    diff.find_similar(None)?;

    let show_summary = !self.no_summary.unwrap_or(!config.show_summary()?);
    if show_summary {
      let summary = DiffSummary::new(&diff)?;
      if summary.num_files != 0 {
        writeln!(buf, "\n{}", display_summary(&summary, config.nerdfont()?))?;
      }
    }

    let show_patch = !self.no_patch.unwrap_or(!config.show_patch()?);
    if show_patch {
      buf.extend_from_slice(&get_formatted_diff(&diff)?);
    }

    Ok(buf)
  }

  fn show_tag(&self, repo: &Repository, config: &UserConfig, tag: &Tag) -> Result<Vec<u8>> {
    use std::io::Write;
    let mut buf: Vec<u8> = Vec::new();

    let opts = DisplayCommitOptions {
      message: self.message.unwrap_or(config.show_message()?),
      time: DisplayTimeOptions {
        relative: config.format_relative()?,
        fmt: config.format_date()?,
      },
    };

    write!(buf, "{}", display_tag(tag, &opts)?)?;

    let obj = tag.peel()?;
    if let Some(kind) = obj.kind() {
      match kind {
        // if it points to a commit, display that commit
        git2::ObjectType::Commit => {
          let commit = obj.as_commit().unwrap();
          write!(buf, "\n\n{}", display_commit(commit, &opts)?)?;

          let parent = commit.parent(0).not_found_ok()?;

          let new_tree = commit.tree()?;
          let old_tree = match parent {
            Some(it) => Some(it.tree()?),
            None => None,
          };

          let mut diff = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)?;
          diff.find_similar(None)?;

          let show_summary = !self.no_summary.unwrap_or(!config.show_summary()?);
          if show_summary {
            let summary = DiffSummary::new(&diff)?;
            if summary.num_files != 0 {
              writeln!(buf, "\n{}", display_summary(&summary, config.nerdfont()?))?;
            }
          }

          let show_patch = !self.no_patch.unwrap_or(!config.show_patch()?);
          if show_patch {
            buf.extend_from_slice(&get_formatted_diff(&diff)?);
          }
        }

        // else show nothing
        git2::ObjectType::Any
        | git2::ObjectType::Tree
        | git2::ObjectType::Blob
        | git2::ObjectType::Tag => {}
      }
    }

    Ok(buf)
  }
}
