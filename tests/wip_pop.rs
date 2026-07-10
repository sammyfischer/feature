use std::fs;

use crate::common::TestRepo;

mod common;

/// Feature should apply the wip changes
#[test]
fn applies_changes() {
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

  repo.feature(&["wip", "pop"]).success();

  // changes applied
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "B\n",
    "Wip contents should be applied"
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

/// Feature should keep the wip entry if "--keep" was specified
#[test]
fn keeps_entry() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.write_file(file_name, "B\n");

  repo.feature(&["wip", "push", "wip on main"]).success();

  // double check that wip worked
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "A",
    "File contents should be reset by push"
  );

  repo.feature(&["wip", "pop", "--keep"]).success();

  // changes applied
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "B\n",
    "Wip contents should be applied"
  );

  // wip ref was not deleted
  assert!(
    repo.path().join(".git/refs/feature/wips/main").exists(),
    "Wip ref should not be deleted"
  );

  // wip reflog was not deleted
  assert!(
    repo
      .path()
      .join(".git/logs/refs/feature/wips/main")
      .exists(),
    "Wip reflog should not be deleted"
  );
}

/// Feature should apply and drop the wip entry with the specified index
#[test]
fn pops_from_specified_index() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.write_file(file_name, "B\n");
  repo.feature(&["wip", "push", "wip b"]).success();

  repo.write_file(file_name, "C\n");
  repo.feature(&["wip", "push", "wip c"]).success();

  // pop B, not C
  repo.feature(&["wip", "pop", "1"]).success();

  // changes applied
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "B\n",
    "Wip contents should be applied"
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

  // wip c should still be in the reflog, wip b should be gone
  repo
    .git(&["reflog", "--format=%s", "refs/feature/wips/main"])
    .success()
    .stdout("wip c\n");
}

/// Feature should apply the wip even when there are conflicts. The conflicts
/// should be left in the workdir and the wip entry should be kept.
#[test]
fn pops_with_conflicts() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.write_file(file_name, "B\n");

  repo.feature(&["wip", "push", "wip on main"]).success();

  // double check that wip worked
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "A",
    "File contents should be reset by push"
  );

  // create conflicting changes
  repo.write_file(file_name, "C\n");

  repo.feature(&["wip", "pop"]).success();

  // conflict markers left in file
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    r#"<<<<<<< ours
C
=======
B
>>>>>>> theirs
"#,
    "File should contain conflicts"
  );

  // wip ref still exists
  repo
    .git(&[
      "show",
      "--no-patch",
      "--format=%s",
      "refs/feature/wips/main",
    ])
    .success()
    .stdout("wip on main\n");

  // wip entry still exists
  repo
    .git(&["reflog", "--format=%s", "refs/feature/wips/main"])
    .success()
    .stdout("wip on main\n");
}

/// Feature should pop the wip specified by the wip ref
#[test]
fn pop_from_wip_ref() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.write_file(file_name, "B\n");
  repo.feature(&["wip", "push", "wip b"]).success();

  repo.write_file(file_name, "C\n");
  repo.feature(&["wip", "push", "wip c"]).success();

  repo.feature(&["start", "topic"]).success();

  // pop wip b from main
  repo.feature(&["wip", "pop", "main:1"]).success();

  // changes are in workdir
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "B\n",
    "Changes should be applied"
  );

  // wip b is dropped from reflog
  repo
    .git(&["reflog", "--format=%s", "refs/feature/wips/main"])
    .success()
    .stdout("wip c\n");
}
