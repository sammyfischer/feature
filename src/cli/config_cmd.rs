//! Config subcommand

use anyhow::{Result, anyhow};
use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::config;
use crate::util::term::get_user_confirmation;

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

#[derive(Clone, Debug, Subcommand)]
pub enum Args {
  /// Creates a config file with default values
  Create(CreateArgs),

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
pub struct CreateArgs {
  /// Which file to create
  #[arg(long, default_value = "project", conflicts_with = "global")]
  pub which: WhichConfig,

  /// Shorthand for --which=global
  #[arg(short, long, conflicts_with = "which")]
  pub global: bool,
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
  /// Which file to modify
  #[arg(long, default_value = "project", conflicts_with = "global")]
  pub which: WhichConfig,

  /// Shorthand for --which=global
  #[arg(short, long, conflicts_with = "which")]
  pub global: bool,

  /// Use dots to access nested keys, e.g. format.branch
  pub key: String,
  pub value: String,
}

#[derive(clap::Args, Clone, Debug)]
pub struct UnsetArgs {
  /// Which file to modify
  #[arg(long, default_value = "project", conflicts_with = "global")]
  pub which: WhichConfig,

  /// Shorthand for --which=global
  #[arg(short, long, conflicts_with = "which")]
  pub global: bool,

  /// List of keys to unset
  #[arg(trailing_var_arg = true)]
  pub keys: Vec<String>,
}

#[derive(clap::Args, Clone, Debug)]
pub struct ArrayArgs {
  /// Which file to modify
  #[arg(long, default_value = "project", conflicts_with = "global")]
  pub which: WhichConfig,

  /// Shorthand for --which=global
  #[arg(short, long, conflicts_with = "which")]
  pub global: bool,

  /// The key of the array
  pub key: String,

  /// The values to modify (append or remove)
  #[arg(trailing_var_arg = true)]
  pub values: Vec<String>,
}

impl Args {
  pub fn run(&self) -> Result<()> {
    match self {
      Args::Create(args) => self.create(args),
      Args::Get(args) => self.get(args),
      Args::Set(args) => self.set(args),
      Args::Unset(args) => self.unset(args),
      Args::Append(args) => self.append(args),
      Args::Remove(args) => self.remove(args),
    }
  }

  pub fn create(&self, args: &CreateArgs) -> Result<()> {
    let mut which = &args.which;
    if args.global {
      which = &WhichConfig::Global;
    }

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
            "A local config file already exists. Do you want to overwrite it?",
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
        "bases" => toml::Value::from(config.bases.clone()).to_string(),
        "protect" => toml::Value::from(config.protect.clone()).to_string(),
        "format.branch_sep" => config.format.branch_sep.clone(),
        "format.branch" => match config.format.branch {
          Some(ref it) => it.clone(),
          None => "None".to_string(),
        },
        "format.log" => config.format.log.clone(),
        "format.graph" => config.format.graph.clone(),
        _ => {
          eprintln!("{} doesn't exist!", (&**key));
          continue;
        }
      };

      println!("{}: {}", key, value);
    }

    Ok(())
  }

  pub fn set(&self, args: &SetArgs) -> Result<()> {
    let mut which = &args.which;
    if args.global {
      which = &WhichConfig::Global;
    }

    let mut doc = load!(which);

    match &*args.key {
      "default_remote" => doc["default_remote"] = toml_edit::value(&args.value),
      "format.branch_sep" => doc["format"]["branch_sep"] = toml_edit::value(&args.value),
      "format.branch" => doc["format"]["branch"] = toml_edit::value(&args.value),
      "format.log" => doc["format"]["log"] = toml_edit::value(&args.value),
      "format.graph" => doc["format"]["graph"] = toml_edit::value(&args.value),
      it => return Err(anyhow!("Unrecognized key: {}", it)),
    }

    save!(which, doc);
    Ok(())
  }

  pub fn unset(&self, args: &UnsetArgs) -> Result<()> {
    let mut which = &args.which;
    if args.global {
      which = &WhichConfig::Global;
    }

    let mut doc = load!(which);

    for key in &args.keys {
      match doc.remove_entry(key) {
        Some((_, value)) => println!("Removed {} (was {})", key, value.to_string().trim()),
        None => eprintln!("Unrecognized key: {}", key),
      }
    }

    save!(which, doc);
    Ok(())
  }

  pub fn append(&self, args: &ArrayArgs) -> Result<()> {
    // short circuit if no values were specified
    if args.values.is_empty() {
      return Ok(());
    }

    let mut which = &args.which;
    if args.global {
      which = &WhichConfig::Global;
    }

    let mut doc = load!(which);

    // TODO: check that this key is known by the config and should actually be an array
    if !doc.contains_key(&args.key) {
      doc[&args.key] = toml_edit::value(toml_edit::Array::new());
    }

    // get mutable item
    let item = doc
      .get_mut(&args.key)
      .ok_or(anyhow!(format!("Failed to obtain key: {}", args.key)))?;

    // get as mutable array
    let value = item
      .as_array_mut()
      .ok_or(anyhow!(format!("Not an array: {}", args.key)))?;

    // push all values
    for v in &args.values {
      value.push(v);
    }

    save!(which, doc);
    Ok(())
  }

  pub fn remove(&self, args: &ArrayArgs) -> Result<()> {
    // short circuit if no values were specified
    if args.values.is_empty() {
      return Ok(());
    }

    let mut which = &args.which;
    if args.global {
      which = &WhichConfig::Global;
    }

    let mut doc = load!(which);
    // TODO: validate and display an error message
    if !doc.contains_key(&args.key) {
      return Ok(());
    }

    // get mutable item
    let item = doc
      .get_mut(&args.key)
      .ok_or(anyhow!(format!("Failed to obtain key: {}", args.key)))?;

    // get as mutable array
    let value = item
      .as_array_mut()
      .ok_or(anyhow!(format!("Not an array: {}", args.key)))?;

    // retain values not specified by command
    value.retain(|v| match v.as_str() {
      Some(it) => !args.values.contains(&it.to_string()),
      None => true,
    });

    save!(which, doc);
    Ok(())
  }
}
