//! Start subcommand

use std::str::Chars;

use anyhow::{Context, Result, anyhow};
use git2::{ErrorCode, Repository};

use crate::cli::{Cli, get_current_branch, get_current_commit};
use crate::config::Config;
use crate::{data, open_repo};

const NOT_ON_BASE_MSG: &str = r"Must call start from a base branch. You can modify base branches with:

`feature config append bases <BRANCH_NAME>`";

const EMPTY_REPO_MSG: &str =
  r"Cannot call start on an empty repository. Create at least one commit first.";

const FORMAT_HELP_MSG: &str = r"Template replacements (in order):
  %%      -> a literal '%'
  %(user) -> user.name found in git config
  %(base) -> base branch name
  %(sep)  -> the separator used to join WORDS
  %s      -> WORDS joined by the separator";

#[derive(clap::Args, Clone, Debug)]
pub struct Args {
  /// The separator to use when joining words
  #[arg(long)]
  pub sep: Option<String>,

  /// Format specifier for branch name
  #[arg(long, visible_alias = "fmt", long_help = FORMAT_HELP_MSG)]
  pub format: Option<String>,

  /// Just print the branch name, after joining args and performing template replacements
  #[arg(long)]
  pub dry_run: bool,

  /// Words to join together as branch name
  #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
  pub words: Vec<String>,
}

impl Args {
  pub fn run(&self, cli: &Cli) -> Result<()> {
    let repo = open_repo!();

    let base_name = get_current_branch(&repo)?;
    if !cli.config.bases.contains(&base_name) {
      return Err(anyhow!(NOT_ON_BASE_MSG));
    }

    let branch_name = self.build_branch_name(&repo, &cli.config, &base_name)?;
    println!("Creating branch {}\u{2026}", branch_name);

    if self.dry_run {
      return Ok(());
    }

    // find commit to create branch on
    let current_commit = get_current_commit(&repo)
      .expect("Failed to find current commit")
      .ok_or(anyhow!(EMPTY_REPO_MSG))?;

    // create branch
    let branch = repo
      .branch(&branch_name, &current_commit, false)
      .expect("Failed to create branch");

    // get tree to checkout
    let tree = branch
      .get()
      .peel_to_tree()
      .expect("Failed to get branch as tree to checkout");

    // checkout branch
    repo
      .checkout_tree(tree.as_object(), None)
      .expect("Failed to switch to branch");

    // update HEAD
    repo
      .set_head(&format!("refs/heads/{}", branch_name))
      .unwrap_or_else(|_| {
        panic!(
          "Failed to update HEAD to new branch {0}. Run: \
          \
          `git switch {0}`",
          branch_name
        )
      });

    // getting info to modify config
    let base = repo
      .find_branch(&base_name, git2::BranchType::Local)
      .unwrap_or_else(|_| panic!("Failed to get reference to base branch {}", base_name));

    let feature_base_name = {
      // we want the upstream of the base, e.g. refs/remotes/origin/main
      let base_upstream = match base.upstream() {
        Ok(it) => Some(it),
        Err(e) if e.code() == ErrorCode::NotFound => None,
        Err(e) => {
          return Err(
            anyhow!(e).context(format!("Failed to check if {} has an upstream", base_name)),
          );
        }
      };

      match base_upstream {
        Some(it) => it
          .get()
          .name()
          .expect("Failed to get upstream name of base branch")
          .to_string(),

        // if there is no upstream, we can just use the actual base branch
        None => base
          .get()
          .name()
          .expect("Failed to get full refname of base branch")
          .to_string(),
      }
    };

    let mut config = data::git_config(&repo)?;
    data::set_feature_base(&mut config, &branch_name, &feature_base_name)?;

    Ok(())
  }

  fn build_branch_name(
    &self,
    repo: &Repository,
    config: &Config,
    base_name: &str,
  ) -> Result<String> {
    let sep = self.sep.as_ref().unwrap_or(&config.branch_sep);
    let main_part = self.words.join(sep);
    let template = self.format.as_ref().unwrap_or(&config.branch_format);

    // cached value of user.name from git config
    let mut username: Option<String> = None;

    /// State machine states
    #[derive(PartialEq)]
    enum State {
      /// Parsing unescaped characters
      Base,
      /// Parsing the first character after an escape
      FirstEscape,
      /// Parsing the remainder of a variable-length escape
      LongEscape,
    }
    let mut state: State = State::Base;
    let mut out = String::new();
    let mut escape_buf = String::new();

    let mut iter = template.chars();

    while let Some(c) = iter.next() {
      match state {
        State::Base => match c {
          '%' => {
            escape_buf.push(c);
            state = State::FirstEscape;
          }
          _ => out.push(c),
        },

        // we've seen one '%' already
        State::FirstEscape => match c {
          '%' => {
            out.push('%');
            escape_buf.clear();
            state = State::Base;
          }
          's' => {
            out.push_str(&main_part);
            escape_buf.clear();
            state = State::Base;
          }
          '(' => {
            escape_buf.push(c);
            state = State::LongEscape;
          }
          _ => {
            escape_buf.push(c);
            return Err(anyhow!("Unrecognized template replacement: {}", escape_buf));
          }
        },

        // user, base, or sep replacement
        State::LongEscape => match c {
          'u' => {
            // the only possible match is "user", check against remaining chars
            // buffer the escape sequence as it is in the template
            escape_buf.push(c);
            parse_long_escape(&mut iter, &mut escape_buf, "ser)")?;

            match username {
              // use cached value
              Some(ref it) => out.push_str(it),
              // compute and cache value
              None => {
                let signature = repo.signature().context("Failed to get git user name")?;
                let value = signature
                  .name()
                  .expect("Git user name should be valid utf-8")
                  .to_string();
                out.push_str(&value);
                username = Some(value);
              }
            }

            // finished parsing escape, back to regular parsing
            escape_buf.clear();
            state = State::Base;
          }
          'b' => {
            escape_buf.push(c);
            parse_long_escape(&mut iter, &mut escape_buf, "ase)")?;
            out.push_str(base_name);

            escape_buf.clear();
            state = State::Base;
          }
          's' => {
            escape_buf.push(c);
            parse_long_escape(&mut iter, &mut escape_buf, "ep)")?;
            out.push_str(sep);

            escape_buf.clear();
            state = State::Base;
          }
          _ => {
            escape_buf.push(c);
            return Err(recover_long_escape(&mut iter, &mut escape_buf));
          }
        },
      }
    }

    if state == State::FirstEscape || state == State::LongEscape {
      return Err(anyhow!("Unrecognized template replacement: {}", escape_buf));
    }

    Ok(out)
  }
}

/// Parses through iter, checking that each char matches each char in `reference`. Buffers all
/// parsed chars into `escape`. Returns an error if there's a mismatch.
fn parse_long_escape(iter: &mut Chars, escape: &mut String, reference: &str) -> Result<()> {
  let mut ref_iter = reference.chars();

  // iterate and check in tandem
  while let Some(c) = ref_iter.next()
    && let Some(d) = iter.next()
  {
    escape.push(c);
    // if at any point they don't match, we have an invalid escape sequence
    if c != d {
      return Err(recover_long_escape(iter, escape));
    }
  }

  Ok(())
}

/// Read the rest of the invalid long escape sequence.
///
/// Takes the existing buffer `escape` which should have buffered all previous escape chars. Pushes
/// new chars up to and including the next ')' or end of the string.
///
/// Returns an anyhow error with a suitable message containing the bad escape sequence
fn recover_long_escape(iter: &mut Chars, escape: &mut String) -> anyhow::Error {
  for c in iter.by_ref() {
    escape.push(c);
    if c == ')' {
      break;
    }
  }
  anyhow!("Unrecognized template replacement: {}", escape)
}
