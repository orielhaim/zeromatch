use crate::compile::CompiledGlob;
use crate::matcher::matches;
use crate::options::MatchOptions;
use crate::parse::ParseError;

pub struct GlobSetMatcher {
  globs: Vec<CompiledGlob>,
  common_literals: Vec<Vec<u8>>,
}

impl GlobSetMatcher {
  pub fn new(patterns: &[String], opts: MatchOptions) -> Result<Self, ParseError> {
    let mut globs = Vec::with_capacity(patterns.len());
    for p in patterns {
      globs.push(CompiledGlob::new(p, opts.clone())?);
    }
    let common = gather_common_literals(&globs);
    Ok(Self {
      globs,
      common_literals: common,
    })
  }

  #[inline]
  pub fn is_match(&self, input: &[u8]) -> bool {
    if !self.globs.iter().any(|g| g.program.negated) && !self.common_literals.is_empty() {
      let any = self
        .common_literals
        .iter()
        .any(|lit| memchr::memmem::find(input, lit).is_some());
      if !any {
        return false;
      }
    }
    for g in &self.globs {
      if matches(&g.program, &g.opts, input) {
        return true;
      }
    }
    false
  }

  pub fn matched_indices(&self, input: &[u8], out: &mut Vec<u32>) {
    out.clear();
    for (i, g) in self.globs.iter().enumerate() {
      if matches(&g.program, &g.opts, input) {
        out.push(i as u32);
      }
    }
  }
}

fn gather_common_literals(globs: &[CompiledGlob]) -> Vec<Vec<u8>> {
  let mut out = Vec::with_capacity(globs.len());
  for g in globs {
    if g.program.negated {
      return Vec::new();
    }
    let Some((off, len)) = g.program.prefilter else {
      return Vec::new();
    };
    if len < 2 {
      return Vec::new();
    }
    out.push(g.program.pool[off as usize..(off + len) as usize].to_vec());
  }
  out
}
