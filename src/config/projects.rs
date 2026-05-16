use std::path::PathBuf;

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub type ProjectsConfig = IndexMap<String, ProjectEntry>;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectEntry {
  pub url: String,
  pub path: PathBuf,
}
