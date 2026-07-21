use std::fs;

use crate::common::TestRepo;

mod common;

/// Moves a wip from given src to dst
#[test]
fn moves_wip() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.feature(&["start", "topic"]).success();
  repo.write_file(file_name, "B\n");
  repo.feature(&["wip", "push", "wip b"]).success();
  repo.write_file(file_name, "C\n");
  repo.feature(&["wip", "push", "wip c"]).success();

  repo.git(&["switch", "main"]).success();
  repo
    .feature(&["wip", "mv", "--from", "topic:1", "main"])
    .success();

  // changes not applied
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "A",
    "Wip contents should not be applied"
  );

  // main wip now points to wip b
  repo
    .git(&[
      "show",
      "--no-patch",
      "--format=%s",
      "refs/feature/wips/main",
    ])
    .success()
    .stdout("wip b\n");

  // topic wip still points to wip c
  repo
    .git(&[
      "show",
      "--no-patch",
      "--format=%s",
      "refs/feature/wips/topic",
    ])
    .success()
    .stdout("wip c\n");
}

/// Moves current branches top wip to dst
#[test]
fn moves_default() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  repo.init_commit();

  repo.feature(&["start", "topic"]).success();
  repo.write_file(file_name, "B\n");
  repo.feature(&["wip", "push", "wip b"]).success();
  repo.write_file(file_name, "C\n");
  repo.feature(&["wip", "push", "wip c"]).success();

  repo.feature(&["wip", "mv", "main"]).success();

  // changes not applied
  assert_eq!(
    fs::read_to_string(repo.path().join(file_name)).unwrap(),
    "A",
    "Wip contents should not be applied"
  );

  repo
    .git(&[
      "show",
      "--no-patch",
      "--format=%s",
      "refs/feature/wips/main",
    ])
    .success()
    .stdout("wip c\n");

  repo
    .git(&[
      "show",
      "--no-patch",
      "--format=%s",
      "refs/feature/wips/topic",
    ])
    .success()
    .stdout("wip b\n");
}
