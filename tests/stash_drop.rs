use std::fs;

use crate::common::TestRepo;

mod common;

/// Feature should drop the entry without applying the changes
#[test]
fn drops_stash_entry() {
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

  repo.feature(&["stash", "drop"]).success();

  // changes not applied
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "A",
    "Stash contents should not be applied"
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

/// Feature should drop the entry at the specified index
#[test]
fn drops_specified_entry() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.write_file(file_name, "B\n");
  repo.feature(&["stash", "push", "wip b"]).success();

  repo.write_file(file_name, "C\n");
  repo.feature(&["stash", "push", "wip c"]).success();

  // drop wip b
  repo.feature(&["stash", "drop", "1"]).success();

  // changes not applied
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "A",
    "Stash contents should not be applied"
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

  // stash reflog contains wip c, not wip b
  repo
    .git(&["reflog", "--format=%s", "refs/feature/stashes/main"])
    .success()
    .stdout("wip c\n");
}
