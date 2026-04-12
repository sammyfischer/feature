//! Advice and error messages used in multiple places

#[macro_export]
macro_rules! opt_advice {
  ($opt:expr, $advice:path) => {
    if $opt { Some($advice) } else { None }
  };
}

/// Error message when a signature is required
pub const NO_SIGNATURE_MSG: &str = r"Failed to get default signature. You must set them with:

git config user.name <name>
git config user.email <email>";

pub const STATUS_ADVICE: &str = r#"Stage changes with "git add …"
Unstage changes with "git rm --cached …""#;

/// Advice to resolve rebase conflicts
pub const REBASE_CONFLICT_ADVICE: &str = r#"You are in an active rebase. You can resolve conflicts by:
1. Modifying the conflicted files
2. Marking the files as resolved with "git add <file>"
3. Continuing the rebase with "git rebase --continue"

Alternatively, you can:
• Skip applying the current commit with "git rebase --skip"
• Return to your state before the rebase with "git rebase --abort""#;

/// Advice to resolve merge conflicts
pub const MERGE_CONFLICT_ADVICE: &str = r#"You are in an active merge. You can resolve conflicts by:
1. Modifying the conflicted files
2. Marking the files as resolved with "git add <file>"
3. Committing, amending, or running "git merge --continue"

Alternatively, you can:
• Return to your state before the merge with "git merge --abort""#;

/// Advice to resolve cherry-pick conflicts
pub const PICK_CONFLICT_ADVICE: &str = r#"You are in an active cherry-pick. You can resolve conflicts by:
1. Modifying the conflicted files
2. Marking the files as resolved with "git add <file>"
3. Running "git cherry-pick --continue"

Alternatively, you can:
• Skip the conflicting commit with "git cherry-pick --skip"
• Skip all remaining commits with "git cherry-pick --quit"
• Return to your state before the pick with "git cherry-pick --abort""#;

/// Advice to resolve revert conflicts
pub const REVERT_CONFLICT_ADVICE: &str = r#"You are in an active revert. You can resolve conflicts by:
1. Modifying the conflicted files
2. Marking the files as resolved with "git add <file>"
3. Running "git revert --continue"

Alternatively, you can:
• Skip the conflicting commit with "git revert --skip"
• Skip all remaining commits with "git revert --quit"
• Return to your state before the pick with "git revert --abort""#;

/// Advice to handle a bisect
pub const BISECT_ADVICE: &str = r#"You are in an active bisect. To continue:

1. Mark one commit as good and one as bad with "git bisect good/bad <commit>"
2. Follow the prompts, marking each commit it checks out as good/bad.
   • To choose a different commit, use "git reset <commit>" and mark that
     commit as good/bad
   • Or use "git bisect skip" to have git choose another commit
3. Once you find the offending commit, exit the bisect with "git bisect reset"

At any point, you can:
• Exit and clean up the bisect with "git bisect reset"
• View progress with "git bisect visualize" and "git bisect log""#;
