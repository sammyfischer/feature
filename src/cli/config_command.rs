//! Config subcommand

use anyhow::{Context, Result, anyhow};
use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::config;
use crate::util::term::get_user_confirmation;

/// Creates a toml value out of the given value, then stringifies
macro_rules! toml_stringify {
  ($opt:expr) => {
    toml::Value::from($opt).to_string()
  };
}

/// Loads the right config document
macro_rules! load {
  ($which:expr) => {
    match $which {
      WhichConfig::Project | WhichConfig::Local => config::project::load_doc()?,
      WhichConfig::User | WhichConfig::Global => config::user::load_doc()?,
    }
  };
}

/// Saves the document to the right config file
macro_rules! save {
  ($which:expr, $doc:expr) => {
    match $which {
      WhichConfig::Project | WhichConfig::Local => config::project::save($doc)?,
      WhichConfig::User | WhichConfig::Global => config::user::save($doc)?,
    }
  };
}

#[derive(clap::Args, Clone, Debug)]
#[command(about = "Interact with feature config")]
pub struct Args {
  /// Which config file to use
  #[arg(long, default_value = "project", conflicts_with = "global")]
  pub which: WhichConfig,

  /// Shorthand for --which=global
  #[arg(short, long, conflicts_with = "which")]
  pub global: bool,

  #[command(subcommand)]
  pub command: ConfigCommand,
}

#[derive(Clone, Debug, Subcommand)]
pub enum ConfigCommand {
  /// Creates a config file with default values
  Create,

  /// Get the value of some config keys
  Get(GetArgs),

  /// Modify a single config value
  Set(SetArgs),

  /// Delete a config keys from a file
  #[command(visible_aliases = ["del", "delete"])]
  Unset(UnsetArgs),

  /// Append a value to an array
  Append(ArrayArgs),

  /// Remove a value from an array
  Remove(ArrayArgs),
}

#[derive(Clone, Debug, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum WhichConfig {
  /// Project (local) config file
  Project,
  /// Project (local) config file
  Local,
  /// Global (user) config file
  User,
  /// Global (user) config file
  Global,
}

#[derive(clap::Args, Clone, Debug)]
pub struct GetArgs {
  /// The names of the keys to get
  #[arg(trailing_var_arg = true)]
  pub keys: Vec<String>,
}

const SET_LONG_ABOUT: &str = r"Each config key is specified as a flag, allowing you to set multiple at once.
Tip: use `append` and `remove` to modify arrays.";

#[derive(clap::Args, Clone, Debug)]
#[command(long_about = SET_LONG_ABOUT)]
pub struct SetArgs {
  /// Use dots to access nested keys, e.g. format.branch
  pub key: String,
  pub value: String,
}

#[derive(clap::Args, Clone, Debug)]
pub struct UnsetArgs {
  /// List of keys to unset
  #[arg(trailing_var_arg = true)]
  pub keys: Vec<String>,
}

#[derive(clap::Args, Clone, Debug)]
pub struct ArrayArgs {
  /// The key of the array
  pub key: String,

  /// The values to modify (append or remove)
  #[arg(trailing_var_arg = true)]
  pub values: Vec<String>,
}

impl Args {
  pub fn run(&self) -> Result<()> {
    let which = if self.global {
      &WhichConfig::Global
    } else {
      &self.which
    };

    match &self.command {
      ConfigCommand::Create => self.create(which),
      ConfigCommand::Get(args) => self.get(args),
      ConfigCommand::Set(args) => self.set(args, which),
      ConfigCommand::Unset(args) => self.unset(args, which),
      ConfigCommand::Append(args) => self.append(args, which),
      ConfigCommand::Remove(args) => self.remove(args, which),
    }
  }

  pub fn create(&self, which: &WhichConfig) -> Result<()> {
    match which {
      WhichConfig::Project | WhichConfig::Local => {
        // if it already exists, prompt user for confirmation
        if config::project::path().exists() {
          let choice = get_user_confirmation(
            "A local config file already exists. Do you want to overwrite it?",
          )?;

          // user selected no
          if !choice {
            return Ok(());
          }
        }
        config::project::save_default()
      }

      WhichConfig::User | WhichConfig::Global => {
        // if it already exists, prompt user for confirmation
        if config::user::path()?.exists() {
          let choice = get_user_confirmation(
            "A global config file already exists. Do you want to overwrite it?",
          )?;

          // user selected no
          if !choice {
            return Ok(());
          }
        }
        config::user::save_default()
      }
    }
  }

  pub fn get(&self, args: &GetArgs) -> Result<()> {
    let config = config::load()?;

    for key in &args.keys {
      let value = match &**key {
        "default_remote" => config.default_remote.clone(),
        "bases" => toml_stringify!(config.bases.clone()),
        "protect" => toml_stringify!(config.protect.clone()),

        "format.branch_sep" => config.format.branch_sep.clone(),
        "format.branch" => match config.format.branch {
          Some(ref it) => it.clone(),
          None => "None".to_string(),
        },
        "format.log" => config.format.log.clone(),
        "format.graph" => config.format.graph.clone(),
        "format.hour" => config.format.hour.to_string(),
        "format.date" => config.format.date.to_string(),
        "format.timezone" => toml_stringify!(config.format.timezone),
        "format.relative" => toml_stringify!(config.format.relative),

        "advice.status" => toml_stringify!(config.advice.status),
        "advice.rebase" => toml_stringify!(config.advice.rebase),
        "advice.merge" => toml_stringify!(config.advice.merge),
        "advice.cherry_pick" => toml_stringify!(config.advice.cherry_pick),
        "advice.revert" => toml_stringify!(config.advice.revert),
        "advice.bisect" => toml_stringify!(config.advice.bisect),

        key => {
          eprintln!("Unrecognized key: {}", key);
          continue;
        }
      };

      println!("{}: {}", key, value);
    }

    Ok(())
  }

  pub fn set(&self, args: &SetArgs, which: &WhichConfig) -> Result<()> {
    let mut doc = load!(which);

    match &*args.key {
      "default_remote" => doc["default_remote"] = toml_edit::value(&args.value),
      "bases" | "protect" => return Err(anyhow!("Use append/remove to edit array fields")),

      // everything else is section.key
      key => {
        let bad_key = Err(anyhow!("Unrecognized key: {}", key));
        let Some((section, field)) = key.split_once(".") else {
          return bad_key;
        };

        match section {
          "format" => {
            // make sure the table exists as a toml `[section]`
            if !doc.contains_table(section) {
              let mut table = toml_edit::Table::new();
              table.set_implicit(false);
              doc[section] = toml_edit::Item::Table(table);
            }

            let section = doc[section]
              .as_table_mut()
              .with_context(|| format!("Failed to get section {}", section))?;

            let value = match field {
              "branch_sep" | "branch" | "log" | "graph" => toml_edit::value(&args.value),
              _ => return bad_key,
            };

            section[field] = value;
          }

          "advice" => {
            // make sure the table exists as a toml `[section]`
            if !doc.contains_table(section) {
              let mut table = toml_edit::Table::new();
              table.set_implicit(false);
              doc[section] = toml_edit::Item::Table(table);
            }

            let section = doc[section]
              .as_table_mut()
              .with_context(|| format!("Failed to get section {}", section))?;

            let value = match field {
              "status" | "rebase" | "merge" | "cherry_pick" | "revert" | "bisect" => {
                toml_edit::value::<bool>(match &*args.value {
                  "true" => true,
                  "false" => false,
                  val => return Err(anyhow!("Not a boolean: {}", val)),
                })
              }
              _ => return bad_key,
            };

            section[field] = value;
          }

          section => return Err(anyhow!("Unrecognized section: {}", section)),
        }
      }
    }

    save!(which, doc);
    Ok(())
  }

  pub fn unset(&self, args: &UnsetArgs, which: &WhichConfig) -> Result<()> {
    let mut doc = load!(which);

    for key in &args.keys {
      match key.split_once(".") {
        None => match doc.remove_entry(key) {
          Some((key, value)) => println!("Removed {} (was {})", key, value.to_string().trim()),
          None => eprintln!("Unrecognized key: {}", key),
        },

        Some((section, field)) => {
          if !validate_section(section, field) {
            eprintln!("Unrecognized key: {}", key);
            continue;
          }

          let Some(table) = doc[section].as_table_mut() else {
            eprintln!("\"{}\" should be a table", section);
            continue;
          };

          match table.remove_entry(field) {
            Some((key, value)) => println!("Removed {} (was {})", key, value.to_string().trim()),
            None => eprintln!("Unrecognized key: {}.{}", section, field),
          }

          if table.is_empty() && doc.remove_entry(section).is_none() {
            eprintln!("Failed to cleanup empty table {}", section);
          };
        }
      }
    }

    save!(which, doc);
    Ok(())
  }

  pub fn append(&self, args: &ArrayArgs, which: &WhichConfig) -> Result<()> {
    // short circuit if no values were specified
    if args.values.is_empty() {
      return Ok(());
    }

    let mut doc = load!(which);
    let key = &args.key;

    if !validate_array(key) {
      return Err(anyhow!("Unrecognized array key: {}", key));
    }

    // ensure the array exists
    if !doc.contains_key(key) {
      doc[key] = toml_edit::value(toml_edit::Array::new());
    }

    // get mutable item
    let item = doc[key]
      .as_array_mut()
      .ok_or(anyhow!(format!("Failed to get field: {}", key)))?;

    // push all values
    for v in &args.values {
      item.push(v);
    }

    save!(which, doc);
    Ok(())
  }

  pub fn remove(&self, args: &ArrayArgs, which: &WhichConfig) -> Result<()> {
    // short circuit if no values were specified
    if args.values.is_empty() {
      return Ok(());
    }

    let mut doc = load!(which);
    let key = &args.key;

    if !validate_array(key) {
      return Err(anyhow!("Unrecognized array key: {}", key));
    }

    // get mutable item
    let item = doc[key]
      .as_array_mut()
      .ok_or(anyhow!(format!("Failed to get field: {}", key)))?;

    // retain values not specified by command
    item.retain(|v| match v.as_str() {
      Some(it) => !args.values.iter().any(|to_remove| it == to_remove),
      None => true,
    });

    if item.is_empty() && doc.remove_entry(key).is_none() {
      eprintln!("Failed to clean up empty array {}", key)
    }

    save!(which, doc);
    Ok(())
  }
}

/// Whether the section/field pair exists in the config
fn validate_section(section: &str, field: &str) -> bool {
  matches!(
    (section, field),
    (
      "format",
      "branch_sep" | "branch" | "log" | "graph" | "hour" | "date" | "timezone" | "relative"
    ) | (
      "advice",
      "status" | "rebase" | "merge" | "cherry_pick" | "revert" | "bisect"
    )
  )
}

fn validate_array(key: &str) -> bool {
  matches!(key, "bases" | "protect")
}
