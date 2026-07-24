use std::fs;
use std::process::Command;

use assert_cmd::assert::OutputAssertExt;

use crate::common::TestRepo;

mod common;

/// Should add project when --url is specified
#[test]
fn adds_with_url() {
  let repo = TestRepo::new();
  let project_remote = TestRepo::new();
  project_remote.init_commit();

  repo
    .feature(&[
      "project",
      "add",
      "--url",
      project_remote.path().to_str().unwrap(),
      "frontend",
    ])
    .success();

  // check that repo was cloned
  Command::new("git")
    .current_dir(repo.path().join("frontend"))
    .env("HOME", repo.path().parent().unwrap())
    .args(["log", "--pretty=format:%s", "main"])
    .assert()
    .stdout("A");

  // check entry in feature.toml
  assert_eq!(
    fs::read_to_string(repo.path().join("feature.toml")).unwrap(),
    format!(
      r#"[projects]
frontend = {{ url = "{}", path = "frontend" }}
"#,
      project_remote.path().to_str().unwrap()
    ),
    "Parent config should be modified"
  );

  // check metadata file in git dir
  let metadata_file = repo.path().join("frontend/.git/feature-project");
  assert!(
    metadata_file.exists(),
    "Project metadata file \"{}\"should exist",
    metadata_file.to_str().unwrap()
  );

  assert_eq!(
    fs::read_to_string(metadata_file).unwrap(),
    format!("{}/", repo.path().to_str().unwrap()),
    "Project metadata should contain parent path"
  );

  // check entry in gitignore
  assert_eq!(
    fs::read_to_string(repo.path().join(".gitignore")).unwrap(),
    "frontend\n",
    "Parent gitignore should be modified"
  );
}

/// Should add already-existing project with --path is specified
#[test]
fn adds_with_path() {
  let repo = TestRepo::new();
  let module = TestRepo::new();
  module.init_commit();

  // make subrepo first
  repo
    .git(&["clone", module.path().to_str().unwrap(), "frontend"])
    .success();

  repo
    .feature(&["project", "add", "--path", "frontend", "frontend"])
    .success();

  // check that repo was cloned
  Command::new("git")
    .current_dir(repo.path().join("frontend"))
    .env("HOME", repo.path().parent().unwrap())
    .args(["log", "--pretty=format:%s", "main"])
    .assert()
    .stdout("A");

  // check entry in feature.toml
  let config = fs::read_to_string(repo.path().join("feature.toml")).unwrap();
  assert_eq!(
    config,
    format!(
      r#"[projects]
frontend = {{ url = "{}", path = "frontend" }}
"#,
      module.path().to_str().unwrap()
    )
  );

  // check entry in gitignore
  let ignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
  assert_eq!(ignore, "frontend\n");
}

/// Should add project when --url and --path are specified
#[test]
fn adds_with_uri_and_path() {
  let repo = TestRepo::new();
  let module = TestRepo::new();
  module.init_commit();

  repo
    .feature(&[
      "project",
      "add",
      "--url",
      module.path().to_str().unwrap(),
      "--path",
      "modules/frontend",
      "frontend",
    ])
    .success();

  // check that repo was cloned
  Command::new("git")
    .current_dir(repo.path().join("modules/frontend"))
    .env("HOME", repo.path().parent().unwrap())
    .args(["log", "--pretty=format:%s", "main"])
    .assert()
    .stdout("A");

  // check entry in feature.toml
  let config = fs::read_to_string(repo.path().join("feature.toml")).unwrap();
  assert_eq!(
    config,
    format!(
      r#"[projects]
frontend = {{ url = "{}", path = "modules/frontend" }}
"#,
      module.path().to_str().unwrap()
    )
  );

  // check entry in gitignore
  let ignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
  assert_eq!(ignore, "modules/frontend\n");
}

/// When a gitignore file exists with entries but doesn't end in a newline,
/// feature should add one
#[test]
fn writes_newline_to_gitignore() {
  let repo = TestRepo::new();
  let module = TestRepo::new();
  module.init_commit();

  // contains an entry but no newline
  repo.write_file(".gitignore", "node_modules");

  repo
    .feature(&[
      "project",
      "add",
      "--url",
      module.path().to_str().unwrap(),
      "frontend",
    ])
    .success();

  let ignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
  assert_eq!(ignore, "node_modules\nfrontend\n");
}

/// Adding multiple projects should result in a correctly formatted config file
#[test]
fn adds_multiple_projects() {
  let repo = TestRepo::new();
  let frontend = TestRepo::new();
  let backend = TestRepo::new();
  frontend.init_commit();
  backend.init_commit();

  repo
    .feature(&[
      "project",
      "add",
      "--url",
      frontend.path().to_str().unwrap(),
      "frontend",
    ])
    .success();

  repo
    .feature(&[
      "project",
      "add",
      "--url",
      backend.path().to_str().unwrap(),
      "backend",
    ])
    .success();

  // check that repo was cloned
  Command::new("git")
    .current_dir(repo.path().join("frontend"))
    .env("HOME", repo.path().parent().unwrap())
    .args(["log", "--pretty=format:%s", "main"])
    .assert()
    .stdout("A");

  Command::new("git")
    .current_dir(repo.path().join("backend"))
    .env("HOME", repo.path().parent().unwrap())
    .args(["log", "--pretty=format:%s", "main"])
    .assert()
    .stdout("A");

  // check feature.toml
  let config = fs::read_to_string(repo.path().join("feature.toml")).unwrap();
  assert_eq!(
    config,
    format!(
      r#"[projects]
frontend = {{ url = "{}", path = "frontend" }}
backend = {{ url = "{}", path = "backend" }}
"#,
      frontend.path().to_str().unwrap(),
      backend.path().to_str().unwrap()
    )
  );

  // check gitignore
  let ignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
  assert_eq!(ignore, "frontend\nbackend\n");
}

/// Doesn't add entry to gitignore if it's already in it
#[test]
fn doesnt_add_to_gitignore_multiple_times() {
  let repo = TestRepo::new();
  let frontend = TestRepo::new();
  frontend.init_commit();

  repo
    .feature(&[
      "project",
      "add",
      "--url",
      frontend.path().to_str().unwrap(),
      "frontend",
    ])
    .success();

  repo.feature(&["project", "add", "frontend"]).success();

  // but there should only be one entry
  let ignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
  assert_eq!(ignore, "frontend\n");
}
