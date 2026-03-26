#![allow(dead_code)]

use std::fs::{self};
use std::io::Write;
use std::path::Path;

use assert_cmd::assert::Assert;
use assert_cmd::{Command, cargo};
use tempfile::TempDir;

/// Gets stdout of an `Assert` as a string
#[macro_export]
macro_rules! get_stdout {
  ($cmd:expr) => {
    String::from_utf8($cmd.get_output().stdout.clone()).expect("Output should be valid utf-8")
  };

  ($cmd:expr, $msg:literal) => {
    String::from_utf8($cmd.get_output().stdout.clone()).expect($msg)
  };
}

/// Get a path as a string
#[macro_export]
macro_rules! path_str {
  ($path:expr) => {
    $path.to_str().expect("Path should exist")
  };

  ($path:expr, $msg:literal) => {
    $path.to_str().expect($msg)
  };
}

pub struct TestRepo {
  pub dir: TempDir,
}

pub struct TestRemote {
  pub dir: TempDir,
}

impl TestRepo {
  pub fn new() -> Self {
    let mut builder = tempfile::Builder::new();
    builder.prefix("repo-");

    let dir = builder.tempdir().expect("Temp dir should be created");
    let this = Self { dir };
    this.git(&["init", path_str!(this.path())]).success();

    this.git(&["config", "user.name", "test"]).success();
    this
      .git(&["config", "user.email", "test@test.net"])
      .success();

    this
  }

  /// Clones an existing repository
  pub fn new_from(repo: &TestRemote, prefix: &str) -> Self {
    let mut builder = tempfile::Builder::new();
    builder.prefix(prefix);

    let dir = builder.tempdir().expect("Temp dir should be created");
    let this = Self { dir };
    this.git(&["clone", path_str!(repo.path()), "."]).success();

    this.git(&["config", "user.name", "test"]).success();
    this
      .git(&["config", "user.email", "test@test.net"])
      .success();

    this
  }

  /// Creates a repo and a remote. Adds the remote to the local repo with the name "origin"
  pub fn new_with_remote() -> (Self, TestRemote) {
    let local = Self::new();
    let remote = TestRemote::new();

    local
      .git(&[
        "remote",
        "add",
        "origin",
        remote.path().to_str().expect("Dir path should exist"),
      ])
      .success();

    (local, remote)
  }

  /// Run a feature command in the repo dir. Returns an `assert_cmd::Assert`
  pub fn feature(&self, args: &[&str]) -> Assert {
    cargo::cargo_bin_cmd!()
      .current_dir(self.path())
      .args(args)
      .assert()
  }

  pub fn git(&self, args: &[&str]) -> Assert {
    Command::new("git")
      .current_dir(self.path())
      .args(args)
      .assert()
  }

  pub fn path(&self) -> &Path {
    self.dir.path()
  }

  /// Writes a file at the top level of the repo
  pub fn write_file(&self, file_name: &str, contents: &str) {
    fs::write(self.path().join(file_name), contents).expect("File should be written to");
  }

  /// Appends to a file at the top level of the repo
  pub fn append_file(&self, file_name: &str, contents: &str) {
    let mut file = fs::OpenOptions::new()
      .append(true)
      .open(self.path().join(file_name))
      .expect("File should've been opened for appending");
    file
      .write(contents.as_bytes())
      .expect("Contents should have been appended to file");
  }

  /// Stages all changes and commits with the given message
  pub fn commit_all(&self, msg: &str) {
    self.git(&["add", "."]).success();
    self.feature(&["commit", msg]).success();
  }

  /// Creates the file "file.txt" and commits with the message "initial commit"
  pub fn init_commit(&self) {
    let file_name = "file.txt";
    self.write_file(file_name, "hello world");
    self.git(&["add", &file_name]).success();
    self.git(&["commit", "-m", "initial commit"]).success();
  }

  /// Gets a list of branches and their upstream tracking branch, via `git branch
  /// --format='%(refname) %(upstream)'`. The format looks like:
  ///
  /// ```txt
  /// refs/heads/main refs/remotes/origin/main
  /// refs/heads/feature-branch
  /// refs/heads/feature2 refs/remotes/origin/feature2
  /// ```
  pub fn list_branches_and_upstreams(&self) -> String {
    let proc = self
      .git(&["branch", "--format=%(refname) %(upstream)"])
      .success();
    get_stdout!(proc)
  }

  /// Lists just the commit hashes of a particular branch
  pub fn list_commits_on_branch(&self, branch: &str) -> String {
    let proc = self.git(&["log", "--pretty=format:%h", branch]);
    get_stdout!(proc)
  }

  /// Creates a pre-commit hook file with the given script.
  ///
  /// `script` must be valid bash, and shouldn't include the shebang line.
  pub fn create_precommit_hook(&self, script: &str) {
    let file = self.path().join(".git").join("hooks").join("pre-commit");
    fs::write(file, format!("#!/bin/bash\n{}", script)).expect("Pre-commit hook should be written");
  }

  /// Whether a rebase is current active
  pub fn is_rebase_active(&self) -> bool {
    self.path().join(".git").join("rebase-merge").exists()
  }
}

impl TestRemote {
  pub fn new() -> Self {
    let mut builder = tempfile::Builder::new();
    builder.prefix("remote-");

    let dir = builder.tempdir().expect("Temp dir should be created");
    let this = Self { dir };

    Command::new("git")
      .current_dir(this.dir.path())
      .args(["init", "--bare", path_str!(this.path())])
      .assert()
      .success();

    this
  }

  /// Runs commands with the --git-dir argument specified, since this repository is bare
  pub fn git(&self, args: &[&str]) -> Assert {
    Command::new("git")
      .current_dir(self.path())
      .args(["--git-dir", path_str!(self.path())])
      .args(args)
      .assert()
  }

  pub fn path(&self) -> &Path {
    self.dir.path()
  }

  pub fn list_branches(&self) -> String {
    let proc = self.git(&["branch", "--format=%(refname)"]).success();
    get_stdout!(proc)
  }

  /// Lists just the commit hashes of a particular branch
  pub fn list_commits_on_branch(&self, branch: &str) -> String {
    let proc = self.git(&["log", "--pretty=format:%h", branch]);
    get_stdout!(proc)
  }
}
