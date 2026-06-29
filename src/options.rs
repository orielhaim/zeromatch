#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
  Posix,
  Windows,
}

impl Mode {
  #[inline(always)]
  pub fn auto() -> Self {
    #[cfg(target_os = "windows")]
    {
      Mode::Windows
    }
    #[cfg(not(target_os = "windows"))]
    {
      Mode::Posix
    }
  }

  #[inline(always)]
  pub fn is_sep(self, b: u8) -> bool {
    match self {
      Mode::Posix => b == b'/',
      Mode::Windows => b == b'/' || b == b'\\',
    }
  }
}

#[derive(Clone, Debug)]
pub struct MatchOptions {
  pub dot: bool,
  pub nocase: bool,
  pub mode: Mode,
  pub contains: bool,
  pub match_base: bool,
  pub nobrace: bool,
  pub nobracket: bool,
  pub noextglob: bool,
  pub noglobstar: bool,
  pub nonegate: bool,
  pub strict_slashes: bool,
  pub max_brace_depth: u8,
  pub max_length: usize,
}

impl Default for MatchOptions {
  #[inline]
  fn default() -> Self {
    Self {
      dot: false,
      nocase: false,
      mode: Mode::Posix,
      contains: false,
      match_base: false,
      nobrace: false,
      nobracket: false,
      noextglob: false,
      noglobstar: false,
      nonegate: false,
      strict_slashes: false,
      max_brace_depth: 10,
      max_length: 65_536,
    }
  }
}
