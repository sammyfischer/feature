use std::fs;

use crate::common::TestRepo;

mod common;

/// Feature should drop the entry without applying the changes
#[test]
fn drops_wip_entry() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.write_file(file_name, "B\n");

  repo.feature(&["wip", "push", "wip on main"]).success();

  // double check that wip worked
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "A",
    "File contents should be reset by wip"
  );

  repo.feature(&["wip", "drop"]).success();

  // changes not applied
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "A",
    "Wip contents should not be applied"
  );

  // wip ref was deleted
  assert!(
    !repo.path().join(".git/refs/feature/wips/main").exists(),
    "Wip ref should be deleted"
  );

  // wip reflog was deleted
  assert!(
    !repo
      .path()
      .join(".git/logs/refs/feature/wips/main")
      .exists(),
    "Wip reflog should be deleted"
  );
}

/// Feature should drop the entry at the specified index
#[test]
fn drops_specified_entry() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.write_file(file_name, "B\n");
  repo.feature(&["wip", "push", "wip b"]).success();

  repo.write_file(file_name, "C\n");
  repo.feature(&["wip", "push", "wip c"]).success();

  // drop wip b
  repo.feature(&["wip", "drop", "1"]).success();

  // changes not applied
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "A",
    "Wip contents should not be applied"
  );

  // wip ref still points to wip c
  repo
    .git(&[
      "show",
      "--no-patch",
      "--format=%s",
      "refs/feature/wips/main",
    ])
    .success()
    .stdout("wip c\n");

  // wip reflog contains wip c, not wip b
  repo
    .git(&["reflog", "--format=%s", "refs/feature/wips/main"])
    .success()
    .stdout("wip c\n");
}

/// Feature should drop the wip specified by the wip spec
#[test]
fn drop_from_wipspec() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.write_file(file_name, "B\n");
  repo.feature(&["wip", "push", "wip b"]).success();

  repo.write_file(file_name, "C\n");
  repo.feature(&["wip", "push", "wip c"]).success();

  repo.feature(&["start", "topic"]).success();

  // drop wip b from main
  repo.feature(&["wip", "drop", "main:1"]).success();

  // changes are not in workdir
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "A",
    "Changes should not be applied"
  );

  // wip b is dropped from reflog
  repo
    .git(&["reflog", "--format=%s", "refs/feature/wips/main"])
    .success()
    .stdout("wip c\n");
}

/// Feature should drop the top wip when there are multiple wips
#[test]
fn drops_top_wip() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.write_file(file_name, "B\n");
  repo.feature(&["wip", "push", "wip b"]).success();
  repo.write_file(file_name, "C\n");
  repo.feature(&["wip", "push", "wip c"]).success();

  repo.feature(&["wip", "drop"]).success();

  // changes not applied
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "A",
    "Wip contents should not be applied"
  );

  // points to wip b
  repo
    .git(&[
      "show",
      "--no-patch",
      "--format=%s",
      "refs/feature/wips/main",
    ])
    .success()
    .stdout("wip b\n");
}
