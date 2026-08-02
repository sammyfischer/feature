use crate::common::TestRepo;

mod common;

#[test]
fn sets_and_unsets() {
  let (local, _remote) = TestRepo::new_with_remote();
  local.init_commit();

  // sets config
  local.feature(&["protect", "main"]).success();
  local
    .git(&["config", "branch.main.feature-protect"])
    .success()
    .stdout("true\n");

  // unsets config
  local.feature(&["protect", "--unset", "main"]).success();
  local
    .git(&["config", "branch.main.feature-protect"])
    .failure();
}

#[test]
fn prevents_prune() {
  let (local, _remote) = TestRepo::new_with_remote();
  local.init_commit();

  local.git(&["push", "-u", "origin", "main"]).success();

  // create branches. don't commit so that their commit history is identical to
  // main
  for branch in ["feature1", "feature2"] {
    local.feature(&["start", branch]).success();
    local.git(&["switch", "main"]).success();
    local.git(&["push", "-u", "origin", branch]).success();
  }

  local.feature(&["protect", "feature1"]).success();

  local.feature(&["prune"]).success();

  // check that they no longer exist
  let cmd = local.git(&["branch"]).success();
  let text = get_stdout!(cmd);

  // feature1 and its config still exist
  assert!(text.contains("feature1"));
  local
    .git(&["config", "branch.feature1.feature-protect"])
    .success();

  // feature2 and its config are deleted
  assert!(!text.contains("feature2"));
  local
    .git(&["config", "branch.feature2.feature-protect"])
    .failure();
}
