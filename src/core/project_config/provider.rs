use figment::value::{Dict, Map};
use figment::{Error, Profile, Provider};

/// A figment provider that wraps another provider, removing a key
pub struct WithoutKey<P>
where
  P: Provider,
{
  provider: P,
  key: &'static str,
}

impl<P: Provider> WithoutKey<P> {
  pub fn new(key: &'static str, provider: P) -> Self {
    Self { provider, key }
  }
}

impl<P: Provider> Provider for WithoutKey<P> {
  fn metadata(&self) -> figment::Metadata {
    self.provider.metadata()
  }

  fn data(&self) -> Result<Map<Profile, Dict>, Error> {
    let mut data = self.provider.data()?;

    // strip the key from every map/dict in data
    for dict in data.values_mut() {
      dict.remove(self.key);
    }

    Ok(data)
  }

  fn profile(&self) -> Option<Profile> {
    self.provider.profile()
  }
}
