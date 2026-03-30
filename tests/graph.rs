use crate::common::TestRepo;

mod common;

#[test]
fn uses_custom_format() {
  let repo = TestRepo::new();
  let file_name = "file.txt";

  for msg in ["A", "B", "C"] {
    repo.write_file(file_name, msg);
    repo.commit_all(msg);
  }

  let cmd = repo.feature(&["graph", "--format=format:%s"]).success();
  let out = get_stdout!(cmd).trim().to_string();
  let lines = out.lines();
  let mut expected_values = ["C", "B", "A"].iter();

  for line in lines {
    assert!(
      line
        .trim()
        .ends_with(expected_values.next().expect("Ran out of lines early"))
    );
  }
}

#[test]
fn uses_custom_format_from_config() {
  let repo = TestRepo::new();
  let file_name = "file.txt";

  for msg in ["A", "B", "C"] {
    repo.write_file(file_name, msg);
    repo.commit_all(msg);
  }

  repo.write_file(
    "feature.toml",
    r#"[format]
log = "format:%s"
"#,
  );

  let cmd = repo.feature(&["log"]).success();
  assert_eq!(get_stdout!(cmd).trim(), "C\nB\nA");
}
