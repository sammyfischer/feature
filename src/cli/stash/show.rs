use anyhow::{Context, Result};

use crate::App;
use crate::util::diff::DiffSummary;
use crate::util::stash::{display_stash_spec, parse_stash_spec};
use crate::util::string::ToStrLossy;
use crate::util::term::paginate;

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Display info about a stash entry")]
pub struct ShowArgs {
  /// Stash-spec to show
  #[arg(value_name = "STASH_SPEC")]
  spec: Option<String>,
}

impl ShowArgs {
  pub fn run(&self, state: &App) -> Result<()> {
    let repo = &state.repo;

    let (branch, num) = parse_stash_spec(repo, self.spec.as_deref())?;
    let stash_refname = format!("refs/feature/stashes/{}", branch.name());
    let reflog = repo.reflog(&stash_refname)?;

    let commit_id = reflog
      .get(num)
      .with_context(|| format!("Entry {} does not exist", num))?
      .id_new();

    let stash = repo.find_commit(commit_id)?;
    let parent = stash
      .parent(0)
      .expect("Failed to get first parent of stash commit");

    let mut diff = repo.diff_tree_to_tree(Some(&parent.tree()?), Some(&stash.tree()?), None)?;
    diff.find_similar(None)?;

    let summary = DiffSummary::new(&diff)?;

    use std::fmt::Write;
    let mut out = String::new();

    writeln!(out, "Stash {}", display_stash_spec(branch.name(), num))?;

    writeln!(out, "\n{}\n", summary)?;

    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
      writeln!(out, "{}", line.content().to_str_lossy()).is_ok()
    })?;

    // TODO: use configured pager
    paginate(out.as_bytes())?;
    Ok(())
  }
}
