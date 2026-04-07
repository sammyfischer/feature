use crate::common::TestRepo;

mod common;

#[test]
fn updates_all_bases() {
  let (local, remote) = TestRepo::new_with_remote();
  local.init_commit();
  local.feature(&["push"]).success();

  local.write_file(".gitignore", "feature.toml");
  local.git(&["add", "."]).success();
  local.feature(&["commit", "added", "gitignore"]).success();

  let bases = ["dev", "test"];

  // create some extra base branches
  for branch in bases {
    local.git(&["switch", "-c", branch]).success();
    local.write_file(&format!("{}.txt", branch), branch);
    local.git(&["add", "."]).success();
    local.feature(&["commit", "impl", branch]).success();
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
    local2.git(&["add", "."]).success();
    local2.feature(&["commit", "modified", branch]).success();
    local2.feature(&["push"]).success();
  }

  local.feature(&["sync"]);

  for branch in bases {
    assert_eq!(
      local.list_commits_on_branch(branch),
      local2.list_commits_on_branch(branch),
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

  let cmd = local.feature(&["sync"]);
  println!("{}", get_stdout!(cmd));

  assert_eq!(
    local.list_commit_subjects("main"),
    "B\nA",
    "Currently checked-out base should be updated"
  );
}
