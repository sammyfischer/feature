use anyhow::{Context, Result};

use crate::App;
use crate::cli::display::diff::display_summary;
use crate::core::diff::DiffSummary;
use crate::core::string::ToStrLossy;
use crate::core::term::paginate;
use crate::core::wip::{display_wip_spec, get_wip_refname, parse_wip_spec};

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Display info about a wip")]
pub struct ShowArgs {
  /// The wip-spec to show
  #[arg(value_name = "WIP_SPEC")]
  spec: Option<String>,
}

impl ShowArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo = &state.repo;

    let (branch, num) = parse_wip_spec(repo, self.spec.as_deref())?;
    let wip_refname = get_wip_refname(branch.name());
    let reflog = repo.reflog(&wip_refname)?;

    let commit_id = reflog
      .get(num)
      .with_context(|| format!("Entry {} does not exist", num))?
      .id_new();

    let wip = repo.find_commit(commit_id)?;
    let parent = wip
      .parent(0)
      .expect("Failed to get first parent of wip commit");

    let mut diff = repo.diff_tree_to_tree(Some(&parent.tree()?), Some(&wip.tree()?), None)?;
    diff.find_similar(None)?;

    let summary = DiffSummary::new(&diff)?;

    use std::fmt::Write;
    let mut out = String::new();

    writeln!(out, "Wip {}", display_wip_spec(branch.name(), num))?;

    writeln!(out, "\n{}\n", display_summary(&summary))?;

    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
      writeln!(out, "{}", line.content().to_str_lossy()).is_ok()
    })?;

    // TODO: use configured pager
    paginate(out.as_bytes())?;
    Ok(())
  }
}
