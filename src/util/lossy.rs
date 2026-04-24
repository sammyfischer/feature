//! Traits to seamlessly convert bytes to UTF-8 through a lossy conversion

use std::borrow::Cow;

use git2::Buf;

pub trait ToStrLossy {
  fn to_str_lossy<'bytes>(&'bytes self) -> Cow<'bytes, str>;
}

pub trait ToStrLossyOwned {
  fn to_str_lossy_owned(&self) -> String;
}

// Anything that impls [ToStrLossy] gets this automatically
impl<T> ToStrLossyOwned for T
where
  T: ToStrLossy,
{
  #[inline]
  fn to_str_lossy_owned(&self) -> String {
    self.to_str_lossy().to_string()
  }
}

impl ToStrLossy for Buf {
  #[inline]
  fn to_str_lossy<'bytes>(&'bytes self) -> Cow<'bytes, str> {
    String::from_utf8_lossy(self)
  }
}

impl ToStrLossy for [u8] {
  #[inline]
  fn to_str_lossy<'bytes>(&'bytes self) -> Cow<'bytes, str> {
    String::from_utf8_lossy(self)
  }
}

impl ToStrLossy for &[u8] {
  #[inline]
  fn to_str_lossy<'bytes>(&'bytes self) -> Cow<'bytes, str> {
    String::from_utf8_lossy(self)
  }
}

impl ToStrLossy for Vec<u8> {
  #[inline]
  fn to_str_lossy<'bytes>(&'bytes self) -> Cow<'bytes, str> {
    String::from_utf8_lossy(self)
  }
}
