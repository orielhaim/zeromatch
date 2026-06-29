use crate::ast::{ExtKind, Node};
use crate::matcher::{
  MatcherFn, match_anchored_dispatch, match_anywhere_dispatch, match_dotted_extensions,
  match_ends_with_literal, match_globstar_literal, match_globstar_star_suffix,
  match_globstar_star_suffixes, match_literal, match_prefix_star, match_prefix_star_suffix,
  matcher_basename_then, matcher_negate_then,
};
use crate::options::MatchOptions;
use crate::parse::{ParseError, parse};
use crate::util::ByteClass;

#[derive(Clone, Debug)]
pub enum Instr {
  Literal {
    off: u32,
    len: u32,
  },
  AnyChar,
  Star,
  Globstar,
  Sep,
  Class(u32),
  BraceOpen {
    alts: (u32, u32),
    end: u32,
  },
  BraceJumpEnd(u32),
  ExtglobOpen {
    kind: ExtKind,
    alts: (u32, u32),
    end: u32,
  },
  ExtglobJumpEnd(u32),
  BraceEnd,
  ExtglobEnd,
}

#[derive(Clone, Copy, Debug)]
pub struct ExtEntry {
  pub off: u32,
  pub len: u32,
  pub packed: u64,
}

#[derive(Clone, Debug)]
pub enum FastShape {
  None,
  Literal,
  PrefixStar {
    prefix_len: u32,
  },
  PrefixStarSuffix {
    prefix_len: u32,
    suffix_off: u32,
    suffix_len: u32,
  },
  GlobstarLiteral {
    suffix_off: u32,
    suffix_len: u32,
  },
  GlobstarStarSuffix {
    suffix_off: u32,
    suffix_len: u32,
  },
  GlobstarStarSuffixes {
    suffixes_off: u32,
    suffixes_len: u32,
  },
  DottedExtensions {
    exts_off: u32,
    exts_len: u32,
  },
  EndsWithLiteral {
    tail_off: u32,
    tail_len: u32,
    tail_packed: u64,
  },
}

#[derive(Clone, Debug)]
pub struct Program {
  pub code: Vec<Instr>,
  pub pool: Vec<u8>,
  pub classes: Vec<ByteClass>,
  pub alt_table: Vec<u32>,
  pub suffix_table: Vec<(u32, u32)>,
  pub ext_table: Vec<ExtEntry>,
  pub negated: bool,
  pub has_slash: bool,
  pub prefilter: Option<(u32, u32)>,
  pub shape: FastShape,
  pub matcher_fn: MatcherFn,
  pub base_matcher_fn: MatcherFn,
}

#[derive(Clone, Debug)]
pub struct CompiledGlob {
  pub program: Program,
  pub opts: MatchOptions,
}

impl CompiledGlob {
  pub fn new(pattern: &str, opts: MatchOptions) -> Result<Self, ParseError> {
    let ast = parse(pattern, &opts)?;
    let mut c = Compiler {
      code: Vec::with_capacity(16),
      pool: Vec::with_capacity(pattern.len()),
      classes: Vec::new(),
      alt_table: Vec::new(),
      opts: &opts,
      best_literal: None,
    };
    c.emit_seq(&ast.nodes);
    let prefilter = c.best_literal;
    let mut program = Program {
      code: c.code,
      pool: c.pool,
      classes: c.classes,
      alt_table: c.alt_table,
      suffix_table: Vec::new(),
      ext_table: Vec::new(),
      negated: ast.negated,
      has_slash: ast.has_slash,
      prefilter,
      shape: FastShape::None,
      matcher_fn: match_anchored_dispatch,
      base_matcher_fn: match_anchored_dispatch,
    };
    program.shape = detect_shape(&mut program, &opts);

    let base = pick_matcher_fn(&program.shape, &opts);
    program.base_matcher_fn = base;

    let mb = if opts.match_base && !program.has_slash {
      matcher_basename_then(base, opts.mode)
    } else {
      base
    };
    let final_fn = if program.negated {
      matcher_negate_then(mb)
    } else {
      mb
    };
    program.matcher_fn = final_fn;

    Ok(CompiledGlob { program, opts })
  }
}

struct Compiler<'a> {
  code: Vec<Instr>,
  pool: Vec<u8>,
  classes: Vec<ByteClass>,
  alt_table: Vec<u32>,
  opts: &'a MatchOptions,
  best_literal: Option<(u32, u32)>,
}

impl<'a> Compiler<'a> {
  fn emit_seq(&mut self, nodes: &[Node]) {
    for n in nodes {
      self.emit_node(n, true);
    }
  }

  fn emit_node(&mut self, n: &Node, top_level: bool) {
    match n {
      Node::Literal(b) => {
        let off = self.pool.len() as u32;
        if self.opts.nocase {
          for &x in b {
            self.pool.push(crate::util::ascii_lower(x));
          }
        } else {
          self.pool.extend_from_slice(b);
        }
        let len = b.len() as u32;
        if top_level && self.best_literal.map_or(true, |(_, l)| len > l) {
          self.best_literal = Some((off, len));
        }
        self.code.push(Instr::Literal { off, len });
      }
      Node::Sep => self.code.push(Instr::Sep),
      Node::AnyChar => self.code.push(Instr::AnyChar),
      Node::Star => self.code.push(Instr::Star),
      Node::Globstar => self.code.push(Instr::Globstar),
      Node::Class(c) => {
        let idx = self.classes.len() as u32;
        self.classes.push(*c);
        self.code.push(Instr::Class(idx));
      }
      Node::Brace(branches) => {
        let open_pc = self.code.len();
        self.code.push(Instr::BraceOpen {
          alts: (0, 0),
          end: 0,
        });
        let alts_start = self.alt_table.len() as u32;
        let mut jumps: Vec<usize> = Vec::with_capacity(branches.len());
        for (i, br) in branches.iter().enumerate() {
          self.alt_table.push(self.code.len() as u32);
          for n in br {
            self.emit_node(n, false);
          }
          if i + 1 < branches.len() {
            jumps.push(self.code.len());
            self.code.push(Instr::BraceJumpEnd(0));
          }
        }
        let alts_end = self.alt_table.len() as u32;
        self.code.push(Instr::BraceEnd);
        let end_pc = self.code.len() as u32;
        if let Instr::BraceOpen { alts, end } = &mut self.code[open_pc] {
          *alts = (alts_start, alts_end);
          *end = end_pc;
        }
        for j in jumps {
          if let Instr::BraceJumpEnd(e) = &mut self.code[j] {
            *e = end_pc;
          }
        }
      }
      Node::Extglob { kind, branches } => {
        let open_pc = self.code.len();
        self.code.push(Instr::ExtglobOpen {
          kind: *kind,
          alts: (0, 0),
          end: 0,
        });
        let alts_start = self.alt_table.len() as u32;
        let mut jumps: Vec<usize> = Vec::with_capacity(branches.len());
        for (i, br) in branches.iter().enumerate() {
          self.alt_table.push(self.code.len() as u32);
          for n in br {
            self.emit_node(n, false);
          }
          if i + 1 < branches.len() {
            jumps.push(self.code.len());
            self.code.push(Instr::ExtglobJumpEnd(0));
          }
        }
        let alts_end = self.alt_table.len() as u32;
        self.code.push(Instr::ExtglobEnd);
        let end_pc = self.code.len() as u32;
        if let Instr::ExtglobOpen { alts, end, .. } = &mut self.code[open_pc] {
          *alts = (alts_start, alts_end);
          *end = end_pc;
        }
        for j in jumps {
          if let Instr::ExtglobJumpEnd(e) = &mut self.code[j] {
            *e = end_pc;
          }
        }
      }
    }
  }
}

#[inline]
fn pack_extension(pool: &[u8], off: u32, len: u32) -> u64 {
  let bytes = &pool[off as usize..(off + len) as usize];
  let mut buf = [0u8; 8];
  let n = bytes.len().min(8);
  buf[..n].copy_from_slice(&bytes[..n]);
  u64::from_le_bytes(buf)
}

fn detect_shape(prog: &mut Program, _opts: &MatchOptions) -> FastShape {
  let n = prog.code.len();

  if n == 1 {
    if let Instr::Literal { .. } = &prog.code[0] {
      return FastShape::Literal;
    }
  }

  if n >= 2 {
    if let Instr::Star = &prog.code[n - 1] {
      if let Some(plen) = literal_run_len(&prog.code, 0, n - 1) {
        return FastShape::PrefixStar { prefix_len: plen };
      }
    }
  }

  if let Some(star_idx) = find_single_star(&prog.code) {
    let plen = literal_run_len(&prog.code, 0, star_idx);
    let suffix = literal_run_at(&prog.code, star_idx + 1, n, &prog.pool);
    if let (Some(plen), Some((soff, slen))) = (plen, suffix) {
      let suffix_bytes = &prog.pool[soff as usize..(soff + slen) as usize];
      if !suffix_bytes.iter().any(|&b| b == b'/' || b == b'\\') {
        return FastShape::PrefixStarSuffix {
          prefix_len: plen,
          suffix_off: soff,
          suffix_len: slen,
        };
      }
    }
  }

  if n == 3 {
    if let (Instr::Globstar, Instr::Sep, Instr::Literal { off, len }) =
      (&prog.code[0], &prog.code[1], &prog.code[2])
    {
      return FastShape::GlobstarLiteral {
        suffix_off: *off,
        suffix_len: *len,
      };
    }
  }

  if n == 4 {
    if let (Instr::Globstar, Instr::Sep, Instr::Star, Instr::Literal { off, len }) =
      (&prog.code[0], &prog.code[1], &prog.code[2], &prog.code[3])
    {
      let (off, len) = (*off, *len);
      let suffix = &prog.pool[off as usize..(off + len) as usize];
      if !suffix.iter().any(|&b| b == b'/' || b == b'\\') {
        if !suffix.is_empty() && suffix[0] == b'.' {
          let packed = if len as usize <= 8 {
            pack_extension(&prog.pool, off, len)
          } else {
            0
          };
          return FastShape::EndsWithLiteral {
            tail_off: off,
            tail_len: len,
            tail_packed: packed,
          };
        }
        return FastShape::GlobstarStarSuffix {
          suffix_off: off,
          suffix_len: len,
        };
      }
    }
  }

  if n >= 6 {
    let head_ok = matches!(
      (&prog.code[0], &prog.code[1], &prog.code[2]),
      (Instr::Globstar, Instr::Sep, Instr::Star)
    );
    if head_ok {
      let case_a = {
        let i3 = &prog.code[3];
        let i4 = prog.code.get(4);
        if let (
          Instr::Literal {
            off: doff,
            len: dlen,
          },
          Some(Instr::BraceOpen { alts, .. }),
        ) = (i3, i4)
        {
          if *dlen == 1 && prog.pool[*doff as usize] == b'.' {
            Some(*alts)
          } else {
            None
          }
        } else {
          None
        }
      };
      if let Some(alts) = case_a {
        if let Some(range) = collect_brace_lits(prog, alts, true) {
          return FastShape::DottedExtensions {
            exts_off: range.0,
            exts_len: range.1,
          };
        }
      }

      let case_b = {
        if let Instr::BraceOpen { alts, .. } = &prog.code[3] {
          Some(*alts)
        } else {
          None
        }
      };
      if let Some(alts) = case_b {
        let all_dotted = brace_alts_all_dotted(prog, alts);
        if all_dotted {
          if let Some(range) = collect_brace_lits(prog, alts, false) {
            return FastShape::DottedExtensions {
              exts_off: range.0,
              exts_len: range.1,
            };
          }
        }
      }
    }
  }

  if n >= 5 {
    if let (Instr::Globstar, Instr::Sep, Instr::Star, Instr::BraceOpen { alts, .. }) =
      (&prog.code[0], &prog.code[1], &prog.code[2], &prog.code[3])
    {
      let (a, b) = *alts;
      let mut suffixes: Vec<(u32, u32)> = Vec::with_capacity((b - a) as usize);
      let mut ok = true;
      for i in a..b {
        let pc = prog.alt_table[i as usize] as usize;
        if let Some(Instr::Literal { off, len }) = prog.code.get(pc) {
          let next = prog.code.get(pc + 1);
          let terminates = matches!(next, Some(Instr::BraceJumpEnd(_)) | Some(Instr::BraceEnd));
          let suffix = &prog.pool[*off as usize..(*off + *len) as usize];
          if !terminates || suffix.iter().any(|&x| x == b'/' || x == b'\\') {
            ok = false;
            break;
          }
          suffixes.push((*off, *len));
        } else {
          ok = false;
          break;
        }
      }
      if ok && !suffixes.is_empty() {
        let start = prog.suffix_table.len() as u32;
        prog.suffix_table.extend(suffixes);
        let endx = prog.suffix_table.len() as u32;
        return FastShape::GlobstarStarSuffixes {
          suffixes_off: start,
          suffixes_len: endx - start,
        };
      }
    }
  }

  FastShape::None
}

fn brace_alts_all_dotted(prog: &Program, alts: (u32, u32)) -> bool {
  let (a, b) = alts;
  if a == b {
    return false;
  }
  for i in a..b {
    let pc = prog.alt_table[i as usize] as usize;
    let Some(Instr::Literal { off, len }) = prog.code.get(pc) else {
      return false;
    };
    let next = prog.code.get(pc + 1);
    if !matches!(next, Some(Instr::BraceJumpEnd(_)) | Some(Instr::BraceEnd)) {
      return false;
    }
    let bytes = &prog.pool[*off as usize..(*off + *len) as usize];
    if bytes.is_empty() || bytes[0] != b'.' {
      return false;
    }
    if bytes.iter().any(|&x| x == b'/' || x == b'\\') {
      return false;
    }
  }
  true
}

fn collect_brace_lits(
  prog: &mut Program,
  alts: (u32, u32),
  prepend_dot: bool,
) -> Option<(u32, u32)> {
  let (a, b) = alts;

  let mut staged: Vec<Vec<u8>> = Vec::with_capacity((b - a) as usize);
  for i in a..b {
    let pc = prog.alt_table[i as usize] as usize;
    let Some(Instr::Literal { off, len }) = prog.code.get(pc) else {
      return None;
    };
    let next = prog.code.get(pc + 1);
    if !matches!(next, Some(Instr::BraceJumpEnd(_)) | Some(Instr::BraceEnd)) {
      return None;
    }
    let bytes = &prog.pool[*off as usize..(*off + *len) as usize];
    if bytes.iter().any(|&c| c == b'/' || c == b'\\') {
      return None;
    }
    let mut owned = Vec::with_capacity(bytes.len() + 1);
    if prepend_dot {
      owned.push(b'.');
    }
    owned.extend_from_slice(bytes);
    staged.push(owned);
  }
  if staged.is_empty() {
    return None;
  }

  let table_start = prog.ext_table.len() as u32;
  for owned in &staged {
    let new_off = prog.pool.len() as u32;
    prog.pool.extend_from_slice(owned);
    let new_len = owned.len() as u32;
    let packed = pack_extension(&prog.pool, new_off, new_len);
    prog.ext_table.push(ExtEntry {
      off: new_off,
      len: new_len,
      packed,
    });
  }
  let table_end = prog.ext_table.len() as u32;
  Some((table_start, table_end - table_start))
}

fn literal_run_at(code: &[Instr], start: usize, end: usize, _pool: &[u8]) -> Option<(u32, u32)> {
  if end - start == 1 {
    if let Instr::Literal { off, len } = &code[start] {
      return Some((*off, *len));
    }
  }
  None
}

fn literal_run_len(code: &[Instr], start: usize, end: usize) -> Option<u32> {
  let mut total = 0u32;
  for i in start..end {
    match &code[i] {
      Instr::Literal { len, .. } => total += *len,
      Instr::Sep => total += 1,
      _ => return None,
    }
  }
  Some(total)
}

fn find_single_star(code: &[Instr]) -> Option<usize> {
  let mut found: Option<usize> = None;
  for (i, ins) in code.iter().enumerate() {
    match ins {
      Instr::Star => {
        if found.is_some() {
          return None;
        }
        found = Some(i);
      }
      Instr::Globstar
      | Instr::AnyChar
      | Instr::Class(_)
      | Instr::BraceOpen { .. }
      | Instr::BraceJumpEnd(_)
      | Instr::ExtglobOpen { .. }
      | Instr::ExtglobJumpEnd(_)
      | Instr::BraceEnd
      | Instr::ExtglobEnd => return None,
      Instr::Literal { .. } | Instr::Sep => {}
    }
  }
  found
}

fn pick_matcher_fn(shape: &FastShape, opts: &MatchOptions) -> MatcherFn {
  if opts.contains {
    return match_anywhere_dispatch;
  }
  match shape {
    FastShape::Literal => match_literal,
    FastShape::PrefixStar { .. } => match_prefix_star,
    FastShape::PrefixStarSuffix { .. } => match_prefix_star_suffix,
    FastShape::GlobstarLiteral { .. } => match_globstar_literal,
    FastShape::GlobstarStarSuffix { .. } => match_globstar_star_suffix,
    FastShape::GlobstarStarSuffixes { .. } => match_globstar_star_suffixes,
    FastShape::DottedExtensions { .. } => match_dotted_extensions,
    FastShape::EndsWithLiteral { .. } => match_ends_with_literal,
    FastShape::None => match_anchored_dispatch,
  }
}
