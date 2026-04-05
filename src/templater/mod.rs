//! An extremely simple template replacement language.
//!
//! The template replacer scans through a string, replacing the variables with their acutal values.
//! It does so by pushing characters as they appear, until reaching a variable. It then looks up the
//! replacement value and pushes that to the output. The replacer's error handling behavior can be
//! customized to either:
//!
//! # Template variables
//!
//! There are two types of template variables: short and long. Short replacements are a `%` followed
//! by a single character. Long replacements are a `%` followed by parenthesized characters, e.g.
//! `%(replacement)`.
//!
//! There is one special case: `%%`. This is a short replacement that always evaluates to a `%` in
//! the output string. Consequently, `%` cannot be used as a user-defined short variable.
//!
//! # Error handling
//!
//! The behavior of the replacer when it encounters an unrecognized variable can be customized to:
//! 1. Fail immediately
//! 2. Continue building the string, but collect errors (containing the invalid variable name and
//!    position in the resulting string)
//! 3. Treat the invalid variable as literal characters and push them to the outupt string

use std::collections::HashMap;

use anyhow::{Result, anyhow};

struct EagerReplacement(String);

struct LazyReplacement<'values> {
  value: Option<String>,
  getter: Box<dyn Fn() -> String + 'values>,
}

trait Replace<'values> {
  fn replace(&mut self) -> &str;
}

impl<'values> Replace<'values> for EagerReplacement {
  fn replace(&mut self) -> &str {
    &self.0
  }
}

impl<'values> Replace<'values> for LazyReplacement<'values> {
  fn replace(&mut self) -> &str {
    match self.value {
      Some(ref it) => it,
      None => {
        self.value = Some((self.getter)());
        self.value.as_ref().unwrap()
      }
    }
  }
}

/// A short variable replacement
pub struct ShortVar<'values> {
  name: char,
  value: Box<dyn Replace<'values> + 'values>,
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

  /// Create a new lazily-evaluated variable. The value is computed on first replacement, and
  /// cached for subsequent replacements. The return value of `replacement` must outlive the
  /// [Templater].
  pub fn lazy(name: char, replacement: impl Fn() -> String + 'values) -> Self {
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
  name: String,
  value: Box<dyn Replace<'values> + 'values>,
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

  /// Create a new lazily-evaluated variable. The value is computed on first replacement, and
  /// cached for subsequent replacements. The return value of `replacement` must outlive the
  /// [Templater].
  pub fn lazy(name: &str, replacement: impl Fn() -> String + 'values) -> Self {
    Self {
      name: name.to_string(),
      value: Box::new(LazyReplacement::<'values> {
        value: None,
        getter: Box::new(replacement),
      }),
    }
  }
}

/// State machine states
#[derive(PartialEq)]
enum State {
  /// Parsing unescaped characters
  Base,
  /// Reached a variable and advanced past the first '%'
  Variable,
  /// Parsing the name of a long variable, after the "%("
  LongVariable,
}

/// An instance of the templater. Use [Templater::new] to create an instance, and chain
/// [Templater::short] and [Templater::long] calls to define variables.
///
/// Use [Templater::replace] to replace the configured variables in the given template. This can be
/// called multiple times for the same templater. For this reason, it's recommended to configure
/// each unique templater once, and reuse it in any context where the same exact variables are
/// supported. It's especially recommended to not define templates in loops.
///
/// The `'values` lifetime is the expected lifetime of any local variable used inside closures for
/// lazily evaluated variables. In general this should be inferred by the compiler, but pay close
/// attention to local variables you use inside closures and make sure they outlive the entire
/// templater instance.
pub struct Templater<'values> {
  short_vars: HashMap<char, Box<dyn Replace<'values> + 'values>>,
  long_vars: HashMap<String, Box<dyn Replace<'values> + 'values>>,
}

#[allow(unused)]
impl<'values> Templater<'values> {
  /// Construct a new instance of a [Templater]
  pub fn new() -> Self {
    Self {
      short_vars: HashMap::new(),
      long_vars: HashMap::new(),
    }
  }

  /// Add a new short variable to the templater. Short variables appear as "%c" in template strings,
  /// where 'c' is the name of the variable.
  ///
  /// Generally, you'll use [ShortVar::eager] or [ShortVar::lazy] as the argument to this function.
  pub fn short(mut self, var: ShortVar<'values>) -> Self {
    self.short_vars.insert(var.name, var.value);
    self
  }

  /// Add a new long variable to the templater. Long variables appear as "%(name)" in template
  /// strings, where "name" is the name of the variable.
  ///
  /// Generally, you'll use [LongVar::eager] or [LongVar::lazy] as the argument to this function.
  pub fn long(mut self, var: LongVar<'values>) -> Self {
    self.long_vars.insert(var.name, var.value);
    self
  }

  /// Build a new string where all variables in the template are replaced with their values.
  ///
  /// Errors if an unrecognized variable is encountered, or if a variable is incomplete. A variable
  /// is incomplete if:
  /// 1. There's a '%' character at the end of the string, e.g. "replacement: %"
  /// 2. There's an unclosed long variable name, e.g. "long variable: %(long"
  pub fn replace(&mut self, template: &str) -> Result<String> {
    // output buffer
    let mut out = String::new();
    // long variable name buffer
    let mut buf = String::new();
    // current state of the state machine
    let mut state = State::Base;

    for c in template.chars() {
      match state {
        State::Base => match c {
          '%' => {
            state = State::Variable;
          }
          _ => out.push(c),
        },

        State::Variable => match c {
          // literal %
          '%' => {
            out.push('%');
            state = State::Base;
          }

          // long variable
          '(' => state = State::LongVariable,

          // short variable
          _ => match self.short_vars.get_mut(&c) {
            Some(value) => {
              out.push_str(value.replace());
              state = State::Base;
            }
            None => return Err(anyhow!("Unrecognized variable: %{}", c)),
          },
        },

        State::LongVariable => match c {
          // finished long var name, perform replacement
          ')' => match self.long_vars.get_mut(&buf) {
            Some(value) => {
              out.push_str(value.replace());
              buf.clear();
              state = State::Base;
            }
            None => return Err(anyhow!("Unrecognized variable: %({})", buf)),
          },

          _ => buf.push(c),
        },
      }
    }

    // reached end of template while parsing a long variable
    if state == State::LongVariable {
      return Err(anyhow!("Unrecognized variable: %({})", buf));
    }

    // template ended with a '%'
    if state == State::Variable {
      return Err(anyhow!("Invalid escape at end of template"));
    }

    Ok(out)
  }
}

#[test]
fn builds_templater() {
  let mut templater = Templater::new()
    .short(ShortVar::eager('l', "<-"))
    .short(ShortVar::eager('r', "->"))
    .long(LongVar::eager("huh", "O_O"));

  assert_eq!(
    templater
      .short_vars
      .get_mut(&'l')
      .expect("Short var 'l' should be mapped")
      .replace(),
    "<-",
    "Short var 'l' should be mapped to '<-'"
  );

  assert_eq!(
    templater
      .short_vars
      .get_mut(&'r')
      .expect("Short var 'r' should be mapped")
      .replace(),
    "->",
    "Short var 'r' should be mapped to '->'"
  );

  assert_eq!(
    templater
      .long_vars
      .get_mut("huh")
      .expect("Long var 'huh' should be mapped")
      .replace(),
    "O_O",
    "Long var 'huh' should be mapped to 'O_O'"
  );
}

#[test]
fn replaces_eager_vars() {
  let mut templater = Templater::new()
    .short(ShortVar::eager('l', "<-"))
    .short(ShortVar::eager('r', "->"))
    .long(LongVar::eager("huh", "O_O"));

  assert_eq!(
    templater
      .replace("%r %(huh) %l")
      .expect("Template should be processed"),
    "-> O_O <-"
  );
}

#[test]
fn replaces_lazy_vars() {
  let mut templater = Templater::new()
    .short(ShortVar::lazy('l', || "<-".to_string()))
    .short(ShortVar::lazy('r', || "->".to_string()))
    .long(LongVar::lazy("huh", || "O_O".to_string()));

  assert_eq!(
    templater
      .replace("%r %(huh) %l")
      .expect("Template should be processed"),
    "-> O_O <-"
  );
}

#[test]
fn replaces_repeated_lazy_vars() {
  let mut templater = Templater::new().short(ShortVar::lazy('l', || "->".to_string()));
  assert_eq!(
    templater
      .replace("%l %l")
      .expect("Template should be processed"),
    "-> ->"
  );
}

#[test]
fn replaces_literal_percent() {
  let mut templater = Templater::new()
    .short(ShortVar::eager('l', "<-"))
    .short(ShortVar::eager('r', "->"))
    .long(LongVar::eager("huh", "O_O"));

  assert_eq!(
    templater
      .replace("%%l should evaluate to %l")
      .expect("Template should be processed"),
    "%l should evaluate to <-"
  );

  assert_eq!(
    templater
      .replace("%(huh) %%(huh)")
      .expect("Template should be processed"),
    "O_O %(huh)"
  );

  assert_eq!(
    templater
      .replace("%%(unrecognized)")
      .expect("Template should be processed"),
    "%(unrecognized)"
  );

  assert_eq!(
    templater
      .replace("%%")
      .expect("Template should be processed"),
    "%"
  );

  assert_eq!(
    templater
      .replace("%%%%")
      .expect("Template should be processed"),
    "%%"
  );
}

#[test]
fn handles_empty_template() {
  let mut templater = Templater::new()
    .short(ShortVar::eager('l', "<-"))
    .short(ShortVar::eager('r', "->"))
    .long(LongVar::eager("huh", "O_O"));

  assert_eq!(
    templater.replace("").expect("Template should be processed"),
    ""
  );
}

#[test]
fn fails_on_unrecognized_short_vars() {
  let mut templater = Templater::new()
    .short(ShortVar::eager('l', "<-"))
    .short(ShortVar::lazy('r', || "->".to_string()))
    .long(LongVar::eager("huh", "O_O"));

  templater
    .replace("%l %s %r")
    .expect_err("Template should fail to process");
}

#[test]
fn fails_on_unrecognized_long_vars() {
  let mut templater = Templater::new()
    .short(ShortVar::eager('l', "<-"))
    .short(ShortVar::lazy('r', || "->".to_string()))
    .long(LongVar::eager("huh", "O_O"));

  templater
    .replace("%l %(huh) %(what) %r")
    .expect_err("Template should fail to process");
}

#[test]
fn fails_on_incomplete_short_vars() {
  let mut templater = Templater::new()
    .short(ShortVar::eager('l', "<-"))
    .short(ShortVar::lazy('r', || "->".to_string()))
    .long(LongVar::eager("huh", "O_O"));

  templater
    .replace("%l %")
    .expect_err("Template should fail to process");
}

#[test]
fn fails_on_incomplete_long_vars() {
  let mut templater = Templater::new()
    .short(ShortVar::eager('l', "<-"))
    .short(ShortVar::lazy('r', || "->".to_string()))
    .long(LongVar::eager("huh", "O_O"));

  templater
    .replace("%l %(hu")
    .expect_err("Template should fail to process");
}
