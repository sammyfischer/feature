use assert_cmd::Command;

use crate::common::TestRepo;

mod common;

#[test]
fn updates_all_bases() {
  let (local, remote) = TestRepo::new_with_remote();
  local.write_file(".gitignore", "feature.toml");
  local.commit_all("A");

  local.feature(&["push"]).success();

  let bases = ["dev", "test"];
  local.write_file("feature.toml", r#"bases = ["main", "dev", "test"]"#);

  // create some extra base branches
  for branch in bases {
    local.git(&["switch", "-c", branch]).success();
    local.write_file(&format!("{}.txt", branch), branch);
    local.commit_all("B");

    local.feature(&["push"]).success();
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

/// Should respect the --no-prune cli option, and the prune = false config option
#[test]
fn respects_no_prune() {
  let (local, _remote) = TestRepo::new_with_remote();
  local.init_commit();

  local.feature(&["start", "topic"]).success();
  local.git(&["push", "-u", "origin", "topic"]).success();

  local.git(&["switch", "main"]).success();
  local.feature(&["sync", "--no-prune"]).success();

  let text = local.list_branches_and_upstreams();
  assert!(
    text.contains("refs/heads/topic refs/remotes/origin/topic"),
    "Sync should respect cli option"
  );

  // now use config option
  local.git(&["config", "feature.sync.prune", "no"]).success();
  local.feature(&["sync"]).success();

  let text = local.list_branches_and_upstreams();
  assert!(
    text.contains("refs/heads/topic refs/remotes/origin/topic"),
    "Sync should respect config file"
  );
}

/// Sync should automatically fetch all branches
#[test]
fn autofetches() {
  let file_name = "file.txt";
  let (local, remote) = TestRepo::new_with_remote();
  local.init_commit();
  local.git(&["push", "-u", "origin", "main"]).success();

  // new commits on main
  let local2 = TestRepo::new_from(&remote, "repo2-");
  local2.write_file(file_name, "B");
  local2.commit_all("B");
  local2.git(&["push", "-f", "origin", "main"]).success();

  // should autofetch and sync new changes
  local.git(&["switch", "main"]).success();
  local.feature(&["sync"]).success();
  assert_eq!(
    local.list_commit_subjects("main"),
    "B\nA",
    "main should have updated"
  );
}

/// Sync should skip automatic fetch when `--no-fetch` is specified
#[test]
fn skips_autofetch_cli() {
  let file_name = "file.txt";
  let (local, remote) = TestRepo::new_with_remote();
  local.init_commit();
  local.git(&["push", "-u", "origin", "main"]).success();

  // new commits on main
  let local2 = TestRepo::new_from(&remote, "repo2-");
  local2.write_file(file_name, "B");
  local2.commit_all("B");
  local2.git(&["push", "-f", "origin", "main"]).success();

  // should autofetch and sync new changes
  local.git(&["switch", "main"]).success();
  local.feature(&["sync", "--no-fetch"]).success();
  assert_eq!(
    local.list_commit_subjects("main"),
    "A",
    "main should not have updated"
  );
}

/// Sync should skip automatic fetch when specified in git config
#[test]
fn skips_autofetch_config() {
  let file_name = "file.txt";
  let (local, remote) = TestRepo::new_with_remote();
  local.init_commit();
  local.git(&["push", "-u", "origin", "main"]).success();

  // new commits on main
  let local2 = TestRepo::new_from(&remote, "repo2-");
  local2.write_file(file_name, "B");
  local2.commit_all("B");
  local2.git(&["push", "-f", "origin", "main"]).success();

  // should autofetch and sync new changes
  local.git(&["switch", "main"]).success();
  local.git(&["config", "feature.autofetch", "no"]).success();
  local.feature(&["sync"]).success();
  assert_eq!(
    local.list_commit_subjects("main"),
    "A",
    "main should not have updated"
  );
}

// Feature should sync all subprojects
#[test]
fn syncs_projects() {
  let file_name = "file.txt";
  let repo = TestRepo::new();
  let frontend = TestRepo::new();
  let backend = TestRepo::new();
  frontend.init_commit();
  backend.init_commit();

  repo
    .feature(&[
      "project",
      "add",
      "--repo",
      frontend.path().to_str().unwrap(),
      "frontend",
    ])
    .success();

  repo
    .feature(&[
      "project",
      "add",
      "--repo",
      backend.path().to_str().unwrap(),
      "backend",
    ])
    .success();

  // new commit to the remotes
  frontend.write_file(file_name, "B");
  frontend.commit_all("B");
  backend.write_file(file_name, "B");
  backend.commit_all("B");

  repo.feature(&["sync"]).success();

  Command::new("git")
    .current_dir(repo.path().join("frontend"))
    .env("HOME", repo.path().parent().unwrap().to_str().unwrap())
    .args(["log", "--pretty=format:%s", "main"])
    .assert()
    .stdout("B\nA");

  Command::new("git")
    .current_dir(repo.path().join("backend"))
    .env("HOME", repo.path().parent().unwrap().to_str().unwrap())
    .args(["log", "--pretty=format:%s", "main"])
    .assert()
    .stdout("B\nA");
}
