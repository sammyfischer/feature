use anyhow::Result;

use crate::templater::replace::{EagerReplacement, LazyReplacement, Replace};

/// A short variable replacement
pub struct ShortVar<'values> {
  pub name: char,
  pub value: Box<dyn Replace<'values> + 'values>,
}

#[allow(unused)]
impl<'values> ShortVar<'values> {
  /// Create a new eagerly-evaluated variable
  pub fn eager(name: char, replacement: &str) -> Self {
    Self {
      name,
      value: Box::new(EagerReplacement(replacement.to_string())),
    }
  }

  /// Create a new lazily-evaluated variable. The value is computed on first
  /// replacement, and cached for subsequent replacements. The return value of
  /// `replacement` must outlive the [Templater].
  pub fn lazy(name: char, replacement: impl Fn() -> Result<String> + 'values) -> Self {
    Self {
      name,
      value: Box::new(LazyReplacement::<'values> {
        value: None,
        getter: Box::new(replacement),
      }),
    }
  }
}

pub struct LongVar<'values> {
  pub name: String,
  pub value: Box<dyn Replace<'values> + 'values>,
}

#[allow(unused)]
impl<'values> LongVar<'values> {
  /// Create a new eagerly-evaluated variable
  pub fn eager(name: &str, replacement: &str) -> Self {
    Self {
      name: name.to_string(),
      value: Box::new(EagerReplacement(replacement.to_string())),
    }
  }

  /// Create a new lazily-evaluated variable. The value is computed on first
  /// replacement, and cached for subsequent replacements. The return value of
  /// `replacement` must outlive the [Templater].
  pub fn lazy(name: &str, replacement: impl Fn() -> Result<String> + 'values) -> Self {
    Self {
      name: name.to_string(),
      value: Box::new(LazyReplacement::<'values> {
        value: None,
        getter: Box::new(replacement),
      }),
    }
  }
}
