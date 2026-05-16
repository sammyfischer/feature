use std::fs;

use assert_cmd::Command;

use crate::common::TestRepo;

mod common;

/// Returns parent repo, frontend remote, backend remote. These are all tempdirs
/// and will be deleted if they leave scope. If you don't need them, bind them
/// to a variable with a real name starting with an underscore.
fn setup_multiproject() -> (TestRepo, TestRepo, TestRepo) {
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

  (repo, frontend, backend)
}

/// Feature should run the command in each subproject and not the parent
#[test]
fn runs_in_each() {
  let (repo, _frontend, _backend) = setup_multiproject();

  fs::create_dir(repo.path().join("frontend/src")).unwrap();

  // will fail in frontend and succeed in backend
  // feature itself will succeed unless a command failed to start
  repo
    .feature(&["project", "each", "mkdir", "src"])
    .success()
    .stdout(
      r#"backend
backend succeeded

frontend
frontend failed exit status: 1
"#,
    );
}

/// Feature should start a branch in each project
#[test]
fn creates_branch_in_each() {
  let (repo, _frontend, _backend) = setup_multiproject();
  let home = repo.path().parent().unwrap();

  repo
    .feature(&["project", "each", "feature", "start", "topic"])
    .success()
    .stdout(
      r#"backend
Created topic (from main)
backend succeeded

frontend
Created topic (from main)
frontend succeeded
"#,
    );

  for project in ["frontend", "backend"] {
    let path = repo.path().join(project);

    // checked out to branch
    Command::new("git")
      .current_dir(&path)
      .env("HOME", home)
      .args(["branch"])
      .assert()
      .success()
      .stdout("  main\n* topic\n");

    // set feature base
    Command::new("git")
      .current_dir(&path)
      .env("HOME", home)
      .args(["config", "branch.topic.feature-base"])
      .assert()
      .success()
      .stdout("refs/remotes/origin/main\n");
  }
}

/// Feature should only exec commands in projects matching the filters
#[test]
fn respects_filter() {
  let (repo, _frontend, _backend) = setup_multiproject();
  let home = repo.path().parent().unwrap();

  let database = TestRepo::new();
  database.init_commit();
  repo
    .feature(&[
      "project",
      "add",
      "--repo",
      database.path().to_str().unwrap(),
      "database",
    ])
    .success();

  repo
    .feature(&[
      "project",
      "each",
      "--filter",
      "frontend,back",
      "feature",
      "start",
      "topic",
    ])
    .success()
    .stdout(
      r#"backend
Created topic (from main)
backend succeeded

frontend
Created topic (from main)
frontend succeeded
"#,
    );

  for project in ["frontend", "backend"] {
    let path = repo.path().join(project);

    // checked out to branch
    Command::new("git")
      .current_dir(&path)
      .env("HOME", home)
      .args(["branch"])
      .assert()
      .success()
      .stdout("  main\n* topic\n");

    // set feature base
    Command::new("git")
      .current_dir(&path)
      .env("HOME", home)
      .args(["config", "branch.topic.feature-base"])
      .assert()
      .success()
      .stdout("refs/remotes/origin/main\n");
  }

  // database shouldn't have the branch
  Command::new("git")
    .current_dir(repo.path().join("database"))
    .env("HOME", home)
    .args(["branch"])
    .assert()
    .success()
    .stdout("* main\n");
}
