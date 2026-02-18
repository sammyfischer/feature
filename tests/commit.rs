use std::fs::write;

use tempfile::TempDir;

use crate::fixtures::*;

fn add_file(dir: &TempDir) {
  let file_name = "file.txt";
  write(dir.path().join(&file_name), "hello world").unwrap();
  run_git(&["add", &file_name], dir.path()).success();
}

#[test]
fn commits() {
  let dir = init_repo();

  // create and add file
  add_file(&dir);

  // commit it
  run_feature(&["commit", "initial", "commit"], dir.path()).success();

  // check latest commit message
  let proc = run_git(&["log", "-1", "--pretty=%B"], dir.path()).success();
  let Ok(stdout) = String::from_utf8(proc.get_output().stdout.clone()) else {
    panic!("Failed to get stdout as string")
  };
  assert_eq!(stdout.trim(), "initial commit".to_string());
}

#[test]
fn no_message_fails() {
  let dir = init_repo();
  add_file(&dir);

  run_feature(&["commit", ""], dir.path()).failure();
}
