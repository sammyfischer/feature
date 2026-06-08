use std::borrow::Cow;

use git2::Buf;

pub trait ToStrLossy {
  /// Converts bytes into a string slice through a lossy conversion
  fn to_str_lossy<'bytes>(&'bytes self) -> Cow<'bytes, str>;
}

pub trait ToStrLossyOwned {
  /// Converts bytes into a string through a lossy conversion
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

pub trait TrimPrefix {
  /// Trims a prefix from a string and returns a slice. If the string doesn't
  /// contain the prefix, returns the whole slice.
  fn trim_prefix_opt(&self, prefix: &str) -> &str;
}

impl TrimPrefix for str {
  fn trim_prefix_opt(&self, prefix: &str) -> &str {
    self.strip_prefix(prefix).unwrap_or(self)
  }
}
