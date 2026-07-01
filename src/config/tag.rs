use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct TagConfig {
  /// Require semver tags to be annotated
  pub require_annotated: bool,
}
