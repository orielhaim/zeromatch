use crate::ast::ExtKind;
use crate::compile::{FastShape, Instr, Program};
use crate::options::{MatchOptions, Mode};

pub type MatcherFn = fn(&Program, &MatchOptions, &[u8]) -> bool;

#[inline(always)]
pub fn matches(prog: &Program, opts: &MatchOptions, input: &[u8]) -> bool {
  (prog.matcher_fn)(prog, opts, input)
}

pub(crate) fn matcher_basename_then(_inner: MatcherFn, mode: Mode) -> MatcherFn {
  match mode {
    Mode::Posix => basename_posix_then_base,
    Mode::Windows => basename_windows_then_base,
  }
}

pub(crate) fn matcher_negate_then(_inner: MatcherFn) -> MatcherFn {
  negate_then_base
}

fn basename_posix_then_base(prog: &Program, opts: &MatchOptions, input: &[u8]) -> bool {
  let view = basename(input, Mode::Posix);
  (prog.base_matcher_fn)(prog, opts, view)
}

fn basename_windows_then_base(prog: &Program, opts: &MatchOptions, input: &[u8]) -> bool {
  let view = basename(input, Mode::Windows);
  (prog.base_matcher_fn)(prog, opts, view)
}

fn negate_then_base(prog: &Program, opts: &MatchOptions, input: &[u8]) -> bool {
  !(prog.base_matcher_fn)(prog, opts, input)
}

#[inline]
pub fn match_literal(prog: &Program, opts: &MatchOptions, input: &[u8]) -> bool {
  let pool = &prog.pool;
  if input.len() != pool.len() {
    return false;
  }
  if opts.nocase {
    eq_lowered(input, pool)
  } else {
    input == pool.as_slice()
  }
}

#[inline]
pub fn match_prefix_star(prog: &Program, opts: &MatchOptions, input: &[u8]) -> bool {
  let FastShape::PrefixStar { prefix_len } = prog.shape else {
    crate::util::cold_path();
    return false;
  };
  let prefix_len = prefix_len as usize;
  if input.len() < prefix_len {
    return false;
  }
  let prefix = &prog.pool[..prefix_len];
  if !starts_with_cased(input, prefix, opts.nocase) {
    return false;
  }
  let rest = &input[prefix_len..];
  let at_segment_start = prefix_len == 0 || opts.mode.is_sep(input[prefix_len - 1]);
  if at_segment_start && !opts.dot && rest.first() == Some(&b'.') && !rest.is_empty() {
    return false;
  }
  !contains_sep(rest, opts.mode)
}

#[inline]
pub fn match_prefix_star_suffix(prog: &Program, opts: &MatchOptions, input: &[u8]) -> bool {
  let FastShape::PrefixStarSuffix {
    prefix_len,
    suffix_off,
    suffix_len,
  } = prog.shape
  else {
    crate::util::cold_path();
    return false;
  };
  let (prefix_len, suffix_off, suffix_len) = (
    prefix_len as usize,
    suffix_off as usize,
    suffix_len as usize,
  );
  if input.len() < prefix_len + suffix_len {
    return false;
  }
  let prefix = &prog.pool[..prefix_len];
  let suffix = &prog.pool[suffix_off..suffix_off + suffix_len];
  if !starts_with_cased(input, prefix, opts.nocase) {
    return false;
  }
  if !ends_with_cased(input, suffix, opts.nocase) {
    return false;
  }
  let middle = &input[prefix_len..input.len() - suffix_len];
  let at_segment_start = prefix_len == 0 || opts.mode.is_sep(input[prefix_len - 1]);
  if at_segment_start && !opts.dot && middle.first() == Some(&b'.') {
    return false;
  }
  !contains_sep(middle, opts.mode)
}

#[inline]
pub fn match_globstar_literal(prog: &Program, opts: &MatchOptions, input: &[u8]) -> bool {
  let FastShape::GlobstarLiteral {
    suffix_off,
    suffix_len,
  } = prog.shape
  else {
    crate::util::cold_path();
    return false;
  };
  let (suffix_off, suffix_len) = (suffix_off as usize, suffix_len as usize);
  let suffix = &prog.pool[suffix_off..suffix_off + suffix_len];
  if input.len() == suffix_len {
    return ends_with_cased(input, suffix, opts.nocase);
  }
  if input.len() < suffix_len + 1 {
    return false;
  }
  let sep_pos = input.len() - suffix_len - 1;
  if !opts.mode.is_sep(input[sep_pos]) {
    return false;
  }
  ends_with_cased(input, suffix, opts.nocase)
}

#[inline]
pub fn match_globstar_star_suffix(prog: &Program, opts: &MatchOptions, input: &[u8]) -> bool {
  let FastShape::GlobstarStarSuffix {
    suffix_off,
    suffix_len,
  } = prog.shape
  else {
    crate::util::cold_path();
    return false;
  };
  let suffix_len = suffix_len as usize;
  if input.len() < suffix_len {
    return false;
  }
  let suffix = &prog.pool[suffix_off as usize..suffix_off as usize + suffix_len];
  if !ends_with_cased(input, suffix, opts.nocase) {
    return false;
  }
  let seg_start = last_seg_start(input, opts.mode);
  globstar_star_suffix_dotrule(input, seg_start, suffix, suffix_len, opts)
}

#[inline]
pub fn match_globstar_star_suffixes(prog: &Program, opts: &MatchOptions, input: &[u8]) -> bool {
  let FastShape::GlobstarStarSuffixes {
    suffixes_off,
    suffixes_len,
  } = prog.shape
  else {
    crate::util::cold_path();
    return false;
  };
  let seg_start = last_seg_start(input, opts.mode);
  let off = suffixes_off as usize;
  let len = suffixes_len as usize;
  let suffixes = unsafe { prog.suffix_table.get_unchecked(off..off + len) };
  for &(soff, slen) in suffixes {
    let suffix_len = slen as usize;
    if input.len() < suffix_len {
      continue;
    }
    let suffix = &prog.pool[soff as usize..soff as usize + suffix_len];
    if !ends_with_cased(input, suffix, opts.nocase) {
      continue;
    }
    if globstar_star_suffix_dotrule(input, seg_start, suffix, suffix_len, opts) {
      return true;
    }
  }
  false
}

#[inline]
pub fn match_ends_with_literal(prog: &Program, opts: &MatchOptions, input: &[u8]) -> bool {
  let FastShape::EndsWithLiteral {
    tail_off,
    tail_len,
    tail_packed,
  } = prog.shape
  else {
    crate::util::cold_path();
    return false;
  };
  let tlen = tail_len as usize;
  if input.len() < tlen {
    return false;
  }
  let seg_start = last_seg_start(input, opts.mode);
  let seg_len = input.len() - seg_start;
  if seg_len < tlen {
    return false;
  }

  let n = input.len();
  let tail_eq = if tlen <= 8 && !opts.nocase {
    let tail_input_packed = if n >= 8 {
      let t8: u64 = unsafe { core::ptr::read_unaligned(input.as_ptr().add(n - 8) as *const u64) };
      let shift_bits = (8 - tlen as u32) * 8;
      let mask = if tlen == 8 {
        !0u64
      } else {
        (1u64 << (tlen as u32 * 8)) - 1
      };
      (t8 >> shift_bits) & mask
    } else {
      let mut buf = [0u8; 8];
      buf[..tlen].copy_from_slice(&input[n - tlen..]);
      u64::from_le_bytes(buf)
    };
    tail_input_packed == tail_packed
  } else if opts.nocase {
    let tail_bytes = &prog.pool[tail_off as usize..tail_off as usize + tlen];
    eq_lowered(&input[n - tlen..], tail_bytes)
  } else {
    let tail_bytes = &prog.pool[tail_off as usize..tail_off as usize + tlen];
    &input[n - tlen..] == tail_bytes
  };
  if !tail_eq {
    return false;
  }

  if !opts.dot && seg_len > 0 && input[seg_start] == b'.' && seg_len != tlen {
    return false;
  }
  true
}

#[inline]
pub fn match_dotted_extensions(prog: &Program, opts: &MatchOptions, input: &[u8]) -> bool {
  let FastShape::DottedExtensions { exts_off, exts_len } = prog.shape else {
    crate::util::cold_path();
    return false;
  };
  if input.is_empty() {
    return false;
  }

  let seg_start = last_seg_start(input, opts.mode);
  let segment = &input[seg_start..];
  if segment.is_empty() {
    return false;
  }
  let starts_dot = segment[0] == b'.';
  let dot_blocked = !opts.dot && starts_dot;

  let off = exts_off as usize;
  let len = exts_len as usize;
  let exts = unsafe { prog.ext_table.get_unchecked(off..off + len) };

  let n = input.len();
  let tail8: u64 = if n >= 8 {
    unsafe { core::ptr::read_unaligned(input.as_ptr().add(n - 8) as *const u64) }
  } else {
    let mut buf = [0u8; 8];
    buf[8 - n..].copy_from_slice(input);
    u64::from_le_bytes(buf)
  };

  for e in exts {
    let elen = e.len as usize;
    if segment.len() < elen {
      continue;
    }
    let matched = if elen <= 8 && !opts.nocase {
      let shift_bits = (8 - elen as u32) * 8;
      let mask = if elen == 8 {
        !0u64
      } else {
        (1u64 << (elen as u32 * 8)) - 1
      };
      let tail_packed_input = (tail8 >> shift_bits) & mask;
      tail_packed_input == e.packed
    } else if opts.nocase {
      let tail = &segment[segment.len() - elen..];
      let ext_bytes = &prog.pool[e.off as usize..e.off as usize + elen];
      eq_lowered(tail, ext_bytes)
    } else {
      let tail = &segment[segment.len() - elen..];
      let ext_bytes = &prog.pool[e.off as usize..e.off as usize + elen];
      tail == ext_bytes
    };
    if !matched {
      continue;
    }

    if dot_blocked {
      continue;
    }
    return true;
  }
  false
}

#[inline(always)]
fn globstar_star_suffix_dotrule(
  input: &[u8],
  seg_start: usize,
  suffix: &[u8],
  suffix_len: usize,
  opts: &MatchOptions,
) -> bool {
  if input.len() - seg_start < suffix_len {
    return false;
  }
  if !opts.dot && input.get(seg_start) == Some(&b'.') {
    let consumed_by_star = input.len() - seg_start - suffix_len;
    if consumed_by_star == 0 {
      if suffix.first() != Some(&b'.') {
        return false;
      }
    } else {
      return false;
    }
  }
  true
}

#[inline]
pub fn match_anchored_dispatch(prog: &Program, opts: &MatchOptions, input: &[u8]) -> bool {
  if let Some((off, len)) = prog.prefilter {
    if len >= 2 {
      let needle = &prog.pool[off as usize..(off + len) as usize];
      let hay_ok = if opts.nocase {
        memmem_nocase(input, needle)
      } else {
        memchr::memmem::find(input, needle).is_some()
      };
      if !hay_ok {
        return false;
      }
    }
  }
  let mut ctx = MatchCtx { prog, opts, input };
  ctx.exec(0, 0, prog.code.len() as u32, true) == MatchResult::Ok(input.len())
}

#[inline]
pub fn match_anywhere_dispatch(prog: &Program, opts: &MatchOptions, input: &[u8]) -> bool {
  let mut ctx = MatchCtx { prog, opts, input };
  let mut i = 0;
  loop {
    if matches!(
      ctx.exec(0, i, prog.code.len() as u32, i == 0),
      MatchResult::Ok(_)
    ) {
      return true;
    }
    if i >= input.len() {
      return false;
    }
    i += 1;
  }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MatchResult {
  Ok(usize),
  Fail,
}

struct MatchCtx<'a> {
  prog: &'a Program,
  opts: &'a MatchOptions,
  input: &'a [u8],
}

impl<'a> MatchCtx<'a> {
  fn exec(&mut self, mut pc: u32, mut pi: usize, end_pc: u32, mut at_start: bool) -> MatchResult {
    while pc < end_pc {
      let instr = unsafe { self.prog.code.get_unchecked(pc as usize) };
      match instr {
        Instr::Literal { off, len } => {
          let need = &self.prog.pool[*off as usize..(*off + *len) as usize];
          if pi + need.len() > self.input.len() {
            return MatchResult::Fail;
          }
          let hay = &self.input[pi..pi + need.len()];
          let ok = if self.opts.nocase {
            eq_lowered(hay, need)
          } else {
            hay == need
          };
          if !ok {
            return MatchResult::Fail;
          }
          at_start = need.last().is_some_and(|&b| self.opts.mode.is_sep(b));
          pi += need.len();
          pc += 1;
        }
        Instr::Sep => {
          if pi >= self.input.len() {
            return MatchResult::Fail;
          }
          if !self.opts.mode.is_sep(self.input[pi]) {
            return MatchResult::Fail;
          }
          pi += 1;
          at_start = true;
          pc += 1;
        }
        Instr::AnyChar => {
          if pi >= self.input.len() {
            return MatchResult::Fail;
          }
          let b = self.input[pi];
          if self.opts.mode.is_sep(b) {
            return MatchResult::Fail;
          }
          if at_start && b == b'.' && !self.opts.dot {
            return MatchResult::Fail;
          }
          pi += 1;
          at_start = false;
          pc += 1;
        }
        Instr::Class(idx) => {
          if pi >= self.input.len() {
            return MatchResult::Fail;
          }
          let b = self.input[pi];
          if self.opts.mode.is_sep(b) {
            return MatchResult::Fail;
          }
          if at_start && b == b'.' && !self.opts.dot {
            return MatchResult::Fail;
          }
          let cls = unsafe { *self.prog.classes.get_unchecked(*idx as usize) };
          let bb = if self.opts.nocase {
            crate::util::ascii_lower(b)
          } else {
            b
          };
          if !cls.contains(bb) {
            return MatchResult::Fail;
          }
          pi += 1;
          at_start = false;
          pc += 1;
        }
        Instr::Star => {
          let next_pc = pc + 1;
          let dot_blocked =
            at_start && pi < self.input.len() && self.input[pi] == b'.' && !self.opts.dot;
          if dot_blocked {
            return self.exec(next_pc, pi, end_pc, at_start);
          }
          if next_pc >= end_pc {
            let mut probe = pi;
            while probe < self.input.len() && !self.opts.mode.is_sep(self.input[probe]) {
              probe += 1;
            }
            return MatchResult::Ok(probe);
          }
          if let Some(Instr::Literal { off, len }) = self.prog.code.get(next_pc as usize) {
            let need = &self.prog.pool[*off as usize..(*off + *len) as usize];
            return self.star_skip(next_pc, pi, end_pc, need);
          }
          let mut max = pi;
          while max < self.input.len() && !self.opts.mode.is_sep(self.input[max]) {
            max += 1;
          }
          let mut probe = max;
          loop {
            if let MatchResult::Ok(end) = self.exec(next_pc, probe, end_pc, false) {
              return MatchResult::Ok(end);
            }
            if probe == pi {
              return MatchResult::Fail;
            }
            probe -= 1;
          }
        }
        Instr::Globstar => {
          let next_pc = pc + 1;
          if next_pc >= end_pc {
            return MatchResult::Ok(self.input.len());
          }

          let (skip_target_pc, needle, leading_sep) = {
            if let Some(Instr::Literal { off, len }) = self.prog.code.get(next_pc as usize) {
              let n = &self.prog.pool[*off as usize..(*off + *len) as usize];
              (Some(next_pc), Some(n), false)
            } else if matches!(self.prog.code.get(next_pc as usize), Some(Instr::Sep)) {
              if let Some(Instr::Literal { off, len }) = self.prog.code.get(next_pc as usize + 1) {
                let n = &self.prog.pool[*off as usize..(*off + *len) as usize];
                (Some(next_pc), Some(n), true)
              } else {
                (None, None, false)
              }
            } else {
              (None, None, false)
            }
          };

          if let (Some(target_pc), Some(n)) = (skip_target_pc, needle) {
            return self.globstar_skip(target_pc, pi, end_pc, n, leading_sep, at_start);
          }

          let followed_by_sep = matches!(self.prog.code.get(next_pc as usize), Some(Instr::Sep));
          let mut probe = pi;
          loop {
            if let MatchResult::Ok(end) = self.exec(next_pc, probe, end_pc, at_start || probe == 0)
            {
              return MatchResult::Ok(end);
            }
            if followed_by_sep {
              let skip_pc = next_pc + 1;
              if skip_pc <= end_pc {
                if let MatchResult::Ok(end) =
                  self.exec(skip_pc, probe, end_pc, at_start || probe == 0)
                {
                  return MatchResult::Ok(end);
                }
              }
            }
            if probe >= self.input.len() {
              return MatchResult::Fail;
            }
            probe += 1;
          }
        }
        Instr::BraceOpen { alts, end } => {
          let (a, b) = *alts;
          let end_after = *end;
          for i in a..b {
            let alt_pc = unsafe { *self.prog.alt_table.get_unchecked(i as usize) };
            if let MatchResult::Ok(end_pi) = self.exec(alt_pc, pi, end_after - 1, at_start) {
              if let MatchResult::Ok(final_end) = self.exec(end_after, end_pi, end_pc, false) {
                return MatchResult::Ok(final_end);
              }
            }
          }
          return MatchResult::Fail;
        }
        Instr::BraceJumpEnd(_) | Instr::BraceEnd => {
          return MatchResult::Ok(pi);
        }
        Instr::ExtglobOpen { kind, alts, end } => {
          let kind = *kind;
          let (a, b) = *alts;
          let end_after = *end;
          let r = self.match_extglob(kind, a, b, end_after - 1, pi, at_start);
          let after_pi = match r {
            MatchResult::Ok(p) => p,
            MatchResult::Fail => return MatchResult::Fail,
          };
          return self.exec(end_after, after_pi, end_pc, false);
        }
        Instr::ExtglobJumpEnd(_) | Instr::ExtglobEnd => {
          return MatchResult::Ok(pi);
        }
      }
    }
    MatchResult::Ok(pi)
  }

  fn star_skip(&mut self, next_pc: u32, pi: usize, end_pc: u32, needle: &[u8]) -> MatchResult {
    let horizon = {
      let mut h = self.input.len();
      for i in pi..self.input.len() {
        if self.opts.mode.is_sep(self.input[i]) {
          h = i;
          break;
        }
      }
      h
    };
    if pi > horizon {
      return MatchResult::Fail;
    }
    let hay = &self.input[pi..horizon];
    let mut search_from = 0usize;
    loop {
      let found = if self.opts.nocase {
        memmem_nocase_from(hay, needle, search_from)
      } else {
        memchr::memmem::find(&hay[search_from..], needle).map(|p| p + search_from)
      };
      let Some(rel) = found else {
        return MatchResult::Fail;
      };
      let probe = pi + rel;
      if let MatchResult::Ok(end) = self.exec(next_pc, probe, end_pc, false) {
        return MatchResult::Ok(end);
      }
      search_from = rel + 1;
      if search_from > hay.len() {
        return MatchResult::Fail;
      }
    }
  }

  fn globstar_skip(
    &mut self,
    target_pc: u32,
    pi: usize,
    end_pc: u32,
    needle: &[u8],
    leading_sep: bool,
    at_start: bool,
  ) -> MatchResult {
    let hay = &self.input[pi..];
    let mut search_from = 0usize;
    loop {
      let found = if self.opts.nocase {
        memmem_nocase_from(hay, needle, search_from)
      } else {
        memchr::memmem::find(&hay[search_from..], needle).map(|p| p + search_from)
      };
      let Some(rel) = found else {
        return MatchResult::Fail;
      };
      let abs = pi + rel;
      let position_ok = if leading_sep {
        (abs == 0) || (abs > 0 && self.opts.mode.is_sep(self.input[abs - 1]))
      } else {
        true
      };
      if position_ok {
        let resume_pc = if leading_sep {
          target_pc + 1
        } else {
          target_pc
        };
        if let MatchResult::Ok(end) = self.exec(resume_pc, abs, end_pc, at_start || abs == 0) {
          return MatchResult::Ok(end);
        }
      }
      search_from = rel + 1;
      if search_from > hay.len() {
        return MatchResult::Fail;
      }
    }
  }

  fn match_extglob(
    &mut self,
    kind: ExtKind,
    a: u32,
    b: u32,
    inner_end: u32,
    pi: usize,
    at_start: bool,
  ) -> MatchResult {
    let alts_ptr = self.prog.alt_table.as_ptr();
    let alts_len = (b - a) as usize;
    let alts_base = unsafe { alts_ptr.add(a as usize) };

    match kind {
      ExtKind::At => {
        for k in 0..alts_len {
          let alt_pc = unsafe { *alts_base.add(k) };
          if let MatchResult::Ok(end) = self.exec(alt_pc, pi, inner_end, at_start) {
            return MatchResult::Ok(end);
          }
        }
        MatchResult::Fail
      }
      ExtKind::Optional => {
        for k in 0..alts_len {
          let alt_pc = unsafe { *alts_base.add(k) };
          if let MatchResult::Ok(end) = self.exec(alt_pc, pi, inner_end, at_start) {
            return MatchResult::Ok(end);
          }
        }
        MatchResult::Ok(pi)
      }
      ExtKind::Star => {
        let mut cur = pi;
        let mut farthest = pi;
        loop {
          let mut advanced = false;
          for k in 0..alts_len {
            let alt_pc = unsafe { *alts_base.add(k) };
            if let MatchResult::Ok(end) = self.exec(alt_pc, cur, inner_end, at_start && cur == pi) {
              if end > cur {
                cur = end;
                advanced = true;
                break;
              }
            }
          }
          if !advanced {
            break;
          }
          farthest = cur;
        }
        MatchResult::Ok(farthest)
      }
      ExtKind::Plus => {
        let mut cur = pi;
        let mut count = 0usize;
        loop {
          let mut advanced = false;
          for k in 0..alts_len {
            let alt_pc = unsafe { *alts_base.add(k) };
            if let MatchResult::Ok(end) = self.exec(alt_pc, cur, inner_end, at_start && cur == pi) {
              if end > cur {
                cur = end;
                count += 1;
                advanced = true;
                break;
              }
            }
          }
          if !advanced {
            break;
          }
        }
        if count == 0 {
          MatchResult::Fail
        } else {
          MatchResult::Ok(cur)
        }
      }
      ExtKind::Negate => {
        let mut cur = pi;
        while cur < self.input.len() && !self.opts.mode.is_sep(self.input[cur]) {
          let mut any = false;
          for k in 0..alts_len {
            let alt_pc = unsafe { *alts_base.add(k) };
            if let MatchResult::Ok(end) = self.exec(alt_pc, pi, inner_end, at_start) {
              if end == cur + 1 {
                any = true;
                break;
              }
            }
          }
          if any {
            return MatchResult::Fail;
          }
          cur += 1;
        }
        let mut empty_any = false;
        for k in 0..alts_len {
          let alt_pc = unsafe { *alts_base.add(k) };
          if let MatchResult::Ok(end) = self.exec(alt_pc, pi, inner_end, at_start) {
            if end == pi {
              empty_any = true;
              break;
            }
          }
        }
        if empty_any {
          return MatchResult::Fail;
        }
        MatchResult::Ok(cur)
      }
    }
  }
}

#[inline]
fn basename(path: &[u8], mode: Mode) -> &[u8] {
  // Use memchr where possible — only on Posix can we do a single-byte search.
  match mode {
    Mode::Posix => {
      if let Some(p) = memchr::memrchr(b'/', path) {
        // SAFETY: `p < path.len()`.
        unsafe { path.get_unchecked(p + 1..) }
      } else {
        path
      }
    }
    Mode::Windows => {
      if let Some(p) = memchr::memrchr2(b'/', b'\\', path) {
        unsafe { path.get_unchecked(p + 1..) }
      } else {
        path
      }
    }
  }
}

#[inline]
fn last_seg_start(input: &[u8], mode: Mode) -> usize {
  match mode {
    Mode::Posix => match memchr::memrchr(b'/', input) {
      Some(p) => p + 1,
      None => 0,
    },
    Mode::Windows => match memchr::memrchr2(b'/', b'\\', input) {
      Some(p) => p + 1,
      None => 0,
    },
  }
}

#[inline]
fn contains_sep(s: &[u8], mode: Mode) -> bool {
  match mode {
    Mode::Posix => memchr::memchr(b'/', s).is_some(),
    Mode::Windows => memchr::memchr2(b'/', b'\\', s).is_some(),
  }
}

#[inline(always)]
fn eq_lowered(a: &[u8], b_lower: &[u8]) -> bool {
  debug_assert_eq!(a.len(), b_lower.len());
  let mut i = 0;
  while i + 8 <= a.len() {
    let av = unsafe { read_u64(a, i) };
    let bv = unsafe { read_u64(b_lower, i) };
    let mask_alpha = ((av ^ 0x4040_4040_4040_4040).wrapping_add(0x1f1f_1f1f_1f1f_1f1f) & !av)
      & 0x8080_8080_8080_8080;
    let folded = av | (mask_alpha >> 2);
    if folded != bv {
      return false;
    }
    i += 8;
  }
  while i < a.len() {
    if crate::util::ascii_lower(a[i]) != b_lower[i] {
      return false;
    }
    i += 1;
  }
  true
}

#[inline(always)]
unsafe fn read_u64(s: &[u8], i: usize) -> u64 {
  let ptr = unsafe { s.as_ptr().add(i) } as *const u64;
  unsafe { core::ptr::read_unaligned(ptr) }
}

#[inline]
fn starts_with_cased(input: &[u8], needle: &[u8], nocase: bool) -> bool {
  if input.len() < needle.len() {
    return false;
  }
  if nocase {
    eq_lowered(&input[..needle.len()], needle)
  } else {
    &input[..needle.len()] == needle
  }
}

#[inline]
fn ends_with_cased(input: &[u8], needle: &[u8], nocase: bool) -> bool {
  if input.len() < needle.len() {
    return false;
  }
  let tail = &input[input.len() - needle.len()..];
  if nocase {
    eq_lowered(tail, needle)
  } else {
    tail == needle
  }
}

fn memmem_nocase(haystack: &[u8], needle_lower: &[u8]) -> bool {
  memmem_nocase_from(haystack, needle_lower, 0).is_some()
}

fn memmem_nocase_from(haystack: &[u8], needle_lower: &[u8], from: usize) -> Option<usize> {
  if needle_lower.is_empty() {
    return Some(from.min(haystack.len()));
  }
  if from + needle_lower.len() > haystack.len() {
    return None;
  }
  let nl = needle_lower[0];
  let nu = if nl.is_ascii_lowercase() {
    nl & !0x20
  } else {
    nl
  };
  let mut i = from;
  while i + needle_lower.len() <= haystack.len() {
    let rel = if nl == nu {
      memchr::memchr(nl, &haystack[i..]).map(|r| r + i)
    } else {
      memchr::memchr2(nl, nu, &haystack[i..]).map(|r| r + i)
    };
    let Some(start) = rel else {
      return None;
    };
    if start + needle_lower.len() > haystack.len() {
      return None;
    }
    if eq_lowered(&haystack[start..start + needle_lower.len()], needle_lower) {
      return Some(start);
    }
    i = start + 1;
  }
  None
}
