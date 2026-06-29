use crate::options::MatchOptions;

#[derive(Debug, Clone)]
pub struct ScanResult {
  pub prefix: String,
  pub base: String,
  pub glob: String,
  pub is_brace: bool,
  pub is_bracket: bool,
  pub is_glob: bool,
  pub is_extglob: bool,
  pub is_globstar: bool,
  pub negated: bool,
  pub negated_extglob: bool,
}

pub fn scan(input: &str, opts: &MatchOptions) -> ScanResult {
  let bytes = input.as_bytes();
  let mut i = 0;
  let mut last_sep = 0usize;
  let mut start = 0usize;
  let mut negated = false;
  let mut negated_extglob = false;
  let mut is_glob = false;
  let mut is_brace = false;
  let mut is_bracket = false;
  let mut is_extglob = false;
  let mut is_globstar = false;
  let mut prefix = String::new();

  if !opts.nonegate {
    while i < bytes.len() && bytes[i] == b'!' && !(i + 1 < bytes.len() && bytes[i + 1] == b'(') {
      negated = !negated;
      i += 1;
      start = i;
    }
  }
  if bytes.get(i) == Some(&b'.') && bytes.get(i + 1).is_some_and(|&b| opts.mode.is_sep(b)) {
    prefix.push('.');
    prefix.push(bytes[i + 1] as char);
    i += 2;
    start = i;
  }

  while i < bytes.len() {
    let b = bytes[i];
    match b {
      b'\\' => {
        i += 2;
        continue;
      }
      x if opts.mode.is_sep(x) => {
        last_sep = i;
      }
      b'?' | b'*' => {
        is_glob = true;
        if b == b'*' && bytes.get(i + 1) == Some(&b'*') {
          is_globstar = true;
        }
        break;
      }
      b'[' => {
        if memchr::memchr(b']', &bytes[i..]).is_some() {
          is_bracket = true;
          is_glob = true;
          break;
        }
      }
      b'{' => {
        is_brace = true;
        is_glob = true;
        break;
      }
      b'@' | b'+' | b'!' | b')' if bytes.get(i + 1) == Some(&b'(') => {
        is_extglob = true;
        is_glob = true;
        if b == b'!' && i == start {
          negated_extglob = true;
        }
        break;
      }
      _ => {}
    }
    i += 1;
  }

  let (base, glob) = if is_glob && last_sep > start {
    (
      input[start..last_sep].to_string(),
      input[last_sep + 1..].to_string(),
    )
  } else if is_glob {
    (String::new(), input[start..].to_string())
  } else {
    (input[start..].to_string(), String::new())
  };

  ScanResult {
    prefix,
    base,
    glob,
    is_brace,
    is_bracket,
    is_glob,
    is_extglob,
    is_globstar,
    negated,
    negated_extglob,
  }
}
