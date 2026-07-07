use std::fs;

use crate::common::TestRepo;

mod common;

/// Feature should apply the stash changes
#[test]
fn applies_changes() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.write_file(file_name, "B\n");

  repo.feature(&["stash", "push", "wip on main"]).success();

  // double check that stash worked
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "A",
    "File contents should be reset by stash"
  );

  repo.feature(&["stash", "pop"]).success();

  // changes applied
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "B\n",
    "Stash contents should be applied"
  );

  // stash ref was deleted
  assert!(
    !repo.path().join(".git/refs/feature/stashes/main").exists(),
    "Stash ref should be deleted"
  );

  // stash reflog was deleted
  assert!(
    !repo
      .path()
      .join(".git/logs/refs/feature/stashes/main")
      .exists(),
    "Stash reflog should be deleted"
  );
}

/// Feature should keep the stash entry if "--keep" was specified
#[test]
fn keeps_entry() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.write_file(file_name, "B\n");

  repo.feature(&["stash", "push", "wip on main"]).success();

  // double check that stash worked
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "A",
    "File contents should be reset by stash"
  );

  repo.feature(&["stash", "pop", "--keep"]).success();

  // changes applied
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "B\n",
    "Stash contents should be applied"
  );

  // stash ref was not deleted
  assert!(
    repo.path().join(".git/refs/feature/stashes/main").exists(),
    "Stash ref should not be deleted"
  );

  // stash reflog was not deleted
  assert!(
    repo
      .path()
      .join(".git/logs/refs/feature/stashes/main")
      .exists(),
    "Stash reflog should not be deleted"
  );
}

/// Feature should apply and drop the stash entry with the specified index
#[test]
fn pops_from_specified_index() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.write_file(file_name, "B\n");
  repo.feature(&["stash", "push", "wip b"]).success();

  repo.write_file(file_name, "C\n");
  repo.feature(&["stash", "push", "wip c"]).success();

  // pop B, not C
  repo.feature(&["stash", "pop", "1"]).success();

  // changes applied
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "B\n",
    "Stash contents should be applied"
  );

  // stash ref still points to wip c
  repo
    .git(&[
      "show",
      "--no-patch",
      "--format=%s",
      "refs/feature/stashes/main",
    ])
    .success()
    .stdout("wip c\n");

  // wip c should still be in the reflog, wip b should be gone
  repo
    .git(&["reflog", "--format=%s", "refs/feature/stashes/main"])
    .success()
    .stdout("wip c\n");
}
