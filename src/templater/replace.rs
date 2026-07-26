use anyhow::Result;

pub struct EagerReplacement(pub String);

pub struct LazyReplacement<'values> {
  pub value: Option<String>,
  pub getter: Box<dyn Fn() -> Result<String> + 'values>,
}

pub trait Replace<'values> {
  fn replace(&mut self) -> Result<&str>;
}

impl<'values> Replace<'values> for EagerReplacement {
  fn replace(&mut self) -> Result<&str> {
    Ok(&self.0)
  }
}

impl<'values> Replace<'values> for LazyReplacement<'values> {
  fn replace(&mut self) -> Result<&str> {
    Ok(match self.value {
      Some(ref it) => it,
      None => {
        self.value = Some((self.getter)()?);
        self.value.as_ref().unwrap()
      }
    })
  }
}
