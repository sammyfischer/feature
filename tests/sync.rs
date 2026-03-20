use crate::common::TestRepo;

mod common;

#[test]
fn updates_all_bases() {
  let (local, remote) = TestRepo::new_with_remote();
  local.init_commit();
  local.feature(&["push"]);

  local.write_file(".gitignore", ".feature.toml");
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

  // sync local and check that they're all updated
  let output = {
    let proc = local.feature(&["sync"]);
    String::from_utf8(proc.get_output().stdout.clone()).expect("Sync output should exist")
  };
  println!("{}", output);

  for branch in bases {
    assert_eq!(
      local.list_commits_on_branch(branch),
      local2.list_commits_on_branch(branch),
      "{} should be the same on local and local2",
      branch
    );
  }

  // local is checked out to main, so it should've been skipped
  assert_ne!(
    local.list_commits_on_branch("main"),
    local2.list_commits_on_branch("main"),
    "main should have been skipped by sync"
  );
}

#[test]
fn fails_if_local_changes_exist() {
  let (local, _remote) = TestRepo::new_with_remote();
  local.init_commit();
  local.feature(&["push"]);

  // add new file to worktree
  local.write_file("new.txt", "uncommitted changes");
  // add to index (new files in worktree always succeed)
  local.git(&["add", "new.txt"]);
  local.feature(&["sync"]).failure();
}
