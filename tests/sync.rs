use crate::common::TestRepo;

mod common;

#[test]
fn updates_all_bases() {
  let (local, remote) = TestRepo::new_with_remote();
  local.write_file(".gitignore", "feature.toml");
  local.commit_all("A");

  local.feature(&["push"]).success();

  let bases = ["dev", "test"];

  // create some extra base branches
  for branch in bases {
    local.git(&["switch", "-c", branch]).success();
    local.write_file(&format!("{}.txt", branch), branch);
    local.commit_all("B");

    local.feature(&["push"]).success();

    local
      .feature(&["config", "append", "bases", branch])
      .success();
    local.git(&["switch", "main"]).success();
  }

  // commit to those from another repo
  let local2 = TestRepo::new_from(&remote, "repo2-");
  for branch in bases {
    local2.git(&["switch", branch]).success();
    local2.write_file(
      &format!("{}-2.txt", branch),
      &format!("added to {}", branch),
    );
    local2.commit_all("C");

    local2.feature(&["push"]).success();
  }

  local.feature(&["sync"]).success();

  for branch in bases {
    assert_eq!(
      local.list_commit_subjects(branch),
      local2.list_commit_subjects(branch),
      "{} should be the same on local and local2",
      branch
    );
  }
}

#[test]
fn updates_current_branch() {
  let (local, remote) = TestRepo::new_with_remote();
  local.write_file("A.txt", "A");
  local.commit_all("A");
  local.feature(&["push"]).success();

  let local2 = TestRepo::new_from(&remote, "repo2-");
  local2.write_file("B.txt", "B");
  local2.commit_all("B");
  local2.feature(&["push"]).success();

  local.feature(&["sync"]).success();

  assert_eq!(
    local.list_commit_subjects("main"),
    "B\nA",
    "Currently checked-out base should be updated"
  );
}

/// Dry run should not update any branches
#[test]
fn dry_run_doesnt_update() {
  let (local, remote) = TestRepo::new_with_remote();
  local.write_file("A.txt", "A");
  local.commit_all("A");
  local.feature(&["push"]).success();

  let local2 = TestRepo::new_from(&remote, "repo2-");
  local2.write_file("B.txt", "B");
  local2.commit_all("B");
  local2.feature(&["push"]).success();

  local.feature(&["sync", "--dry-run"]).success();

  assert_eq!(
    local.list_commit_subjects("main"),
    "A",
    "Main should not be updated"
  );
}
