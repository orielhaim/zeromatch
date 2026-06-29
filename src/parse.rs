use crate::ast::{ExtKind, Node, Pattern};
use crate::options::MatchOptions;
use crate::util::{ByteClass, posix_class};

pub type ParseResult<T> = Result<T, ParseError>;

#[derive(Clone, Debug)]
pub struct ParseError {
  pub message: &'static str,
  pub position: usize,
}

pub fn parse(input: &str, opts: &MatchOptions) -> ParseResult<Pattern> {
  if input.len() > opts.max_length {
    return Err(ParseError {
      message: "pattern exceeds max length",
      position: 0,
    });
  }
  let bytes = input.as_bytes();
  let mut p = Parser {
    bytes,
    pos: 0,
    opts,
  };
  let mut negated = false;
  if !opts.nonegate {
    while p.peek() == Some(b'!') && p.peek_at(1) != Some(b'(') {
      negated = !negated;
      p.pos += 1;
    }
  }
  let nodes = p.parse_seq(Stop::Eof)?;
  let has_slash = contains_slash(&nodes);
  Ok(Pattern {
    nodes,
    negated,
    has_slash,
  })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stop {
  Eof,
  BraceCommaOrClose,
  ExtglobPipeOrClose,
}

struct Parser<'a> {
  bytes: &'a [u8],
  pos: usize,
  opts: &'a MatchOptions,
}

impl<'a> Parser<'a> {
  #[inline(always)]
  fn peek(&self) -> Option<u8> {
    self.bytes.get(self.pos).copied()
  }
  #[inline(always)]
  fn peek_at(&self, off: usize) -> Option<u8> {
    self.bytes.get(self.pos + off).copied()
  }
  #[inline(always)]
  fn bump(&mut self) -> Option<u8> {
    let b = self.peek()?;
    self.pos += 1;
    Some(b)
  }

  fn parse_seq(&mut self, stop: Stop) -> ParseResult<Vec<Node>> {
    let mut out: Vec<Node> = Vec::with_capacity(8);
    let mut lit: Vec<u8> = Vec::new();
    let flush = |lit: &mut Vec<u8>, out: &mut Vec<Node>| {
      if !lit.is_empty() {
        out.push(Node::Literal(std::mem::take(lit)));
      }
    };

    while let Some(b) = self.peek() {
      match stop {
        Stop::Eof => {}
        Stop::BraceCommaOrClose => {
          if b == b',' || b == b'}' {
            break;
          }
        }
        Stop::ExtglobPipeOrClose => {
          if b == b'|' || b == b')' {
            break;
          }
        }
      }
      match b {
        b'\\' => {
          self.pos += 1;
          if let Some(n) = self.bump() {
            let v = match n {
              b'a' => 0x07,
              b'b' => 0x08,
              b'n' => b'\n',
              b'r' => b'\r',
              b't' => b'\t',
              other => other,
            };
            lit.push(v);
          } else {
            lit.push(b'\\');
          }
        }
        b'/' => {
          flush(&mut lit, &mut out);
          self.pos += 1;
          out.push(Node::Sep);
        }
        b'?' => {
          self.pos += 1;
          if !self.opts.noextglob && self.peek() == Some(b'(') {
            flush(&mut lit, &mut out);
            self.pos += 1;
            let branches = self.parse_extglob_branches()?;
            out.push(Node::Extglob {
              kind: ExtKind::Optional,
              branches,
            });
          } else {
            flush(&mut lit, &mut out);
            out.push(Node::AnyChar);
          }
        }
        b'*' => {
          self.pos += 1;
          if !self.opts.noextglob && self.peek() == Some(b'(') {
            flush(&mut lit, &mut out);
            self.pos += 1;
            let branches = self.parse_extglob_branches()?;
            out.push(Node::Extglob {
              kind: ExtKind::Star,
              branches,
            });
          } else {
            flush(&mut lit, &mut out);
            let mut globstar = self.peek() == Some(b'*');
            if globstar {
              self.pos += 1;
            }
            while self.peek() == Some(b'*') {
              globstar = true;
              self.pos += 1;
            }
            if globstar && !self.opts.noglobstar {
              out.push(Node::Globstar);
            } else {
              out.push(Node::Star);
            }
          }
        }
        b'+' => {
          if !self.opts.noextglob && self.peek_at(1) == Some(b'(') {
            flush(&mut lit, &mut out);
            self.pos += 2;
            let branches = self.parse_extglob_branches()?;
            out.push(Node::Extglob {
              kind: ExtKind::Plus,
              branches,
            });
          } else {
            self.pos += 1;
            lit.push(b'+');
          }
        }
        b'@' => {
          if !self.opts.noextglob && self.peek_at(1) == Some(b'(') {
            flush(&mut lit, &mut out);
            self.pos += 2;
            let branches = self.parse_extglob_branches()?;
            out.push(Node::Extglob {
              kind: ExtKind::At,
              branches,
            });
          } else {
            self.pos += 1;
            lit.push(b'@');
          }
        }
        b'!' => {
          if !self.opts.noextglob && self.peek_at(1) == Some(b'(') {
            flush(&mut lit, &mut out);
            self.pos += 2;
            let branches = self.parse_extglob_branches()?;
            out.push(Node::Extglob {
              kind: ExtKind::Negate,
              branches,
            });
          } else {
            self.pos += 1;
            lit.push(b'!');
          }
        }
        b'[' if !self.opts.nobracket => {
          flush(&mut lit, &mut out);
          if let Some(cls) = self.parse_class()? {
            out.push(Node::Class(cls));
          } else {
            lit.push(b'[');
          }
        }
        b'{' if !self.opts.nobrace => {
          flush(&mut lit, &mut out);
          if let Some(brace) = self.parse_brace(0)? {
            out.push(brace);
          } else {
            lit.push(b'{');
          }
        }
        _ => {
          self.pos += 1;
          lit.push(b);
        }
      }
    }
    flush(&mut lit, &mut out);
    Ok(out)
  }

  fn parse_class(&mut self) -> ParseResult<Option<ByteClass>> {
    let start = self.pos;
    self.pos += 1;
    let mut cls = ByteClass::EMPTY;
    let mut negated = false;
    if let Some(b'!' | b'^') = self.peek() {
      negated = true;
      self.pos += 1;
    }
    let mut first = true;
    let mut found_close = false;
    while let Some(b) = self.peek() {
      if b == b']' && !first {
        self.pos += 1;
        found_close = true;
        break;
      }
      if b == b'['
        && self.peek_at(1) == Some(b':')
        && let Some(end) = find_seq(&self.bytes[self.pos..], b":]")
      {
        let name = &self.bytes[self.pos + 2..self.pos + end];
        if let Some(named) = posix_class(name) {
          for w in 0..4 {
            cls.bits[w] |= named.bits[w];
          }
          self.pos += end + 2;
          first = false;
          continue;
        }
      }
      let lo = self.read_class_byte()?;
      let hi = if self.peek() == Some(b'-') && self.peek_at(1) != Some(b']') {
        self.pos += 1;
        self.read_class_byte()?
      } else {
        lo
      };
      cls.add_range(lo, hi);
      first = false;
    }
    if !found_close {
      self.pos = start;
      return Ok(None);
    }
    if negated {
      cls.negate();
      cls.bits[0] &= !(1u64 << b'/');
      if self.opts.mode == crate::options::Mode::Windows {
        cls.bits[1] &= !(1u64 << (b'\\' - 64));
      }
    }
    if self.opts.nocase {
      cls.fold_ascii_case();
    }
    Ok(Some(cls))
  }

  fn read_class_byte(&mut self) -> ParseResult<u8> {
    match self.peek() {
      None => Err(ParseError {
        message: "unterminated class",
        position: self.pos,
      }),
      Some(b'\\') => {
        self.pos += 1;
        if let Some(n) = self.bump() {
          Ok(match n {
            b'a' => 0x07,
            b'b' => 0x08,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            other => other,
          })
        } else {
          Ok(b'\\')
        }
      }
      Some(b) => {
        self.pos += 1;
        Ok(b)
      }
    }
  }

  fn parse_brace(&mut self, depth: u8) -> ParseResult<Option<Node>> {
    if depth >= self.opts.max_brace_depth {
      return Err(ParseError {
        message: "max brace depth exceeded",
        position: self.pos,
      });
    }
    let start = self.pos;
    self.pos += 1;
    if !brace_balanced(&self.bytes[start..]) {
      self.pos = start;
      return Ok(None);
    }
    let mut branches: Vec<Vec<Node>> = Vec::with_capacity(2);
    loop {
      let nodes = self.parse_seq(Stop::BraceCommaOrClose)?;
      branches.push(nodes);
      match self.peek() {
        Some(b',') => {
          self.pos += 1;
          continue;
        }
        Some(b'}') => {
          self.pos += 1;
          break;
        }
        _ => {
          self.pos = start;
          return Ok(None);
        }
      }
    }
    if branches.len() == 1 {
      let mut out = Vec::with_capacity(branches[0].len() + 2);
      out.push(Node::Literal(b"{".to_vec()));
      out.extend(branches.pop().unwrap());
      out.push(Node::Literal(b"}".to_vec()));
      return Ok(Some(Node::Brace(vec![out])));
    }
    Ok(Some(Node::Brace(branches)))
  }

  fn parse_extglob_branches(&mut self) -> ParseResult<Vec<Vec<Node>>> {
    let mut branches: Vec<Vec<Node>> = Vec::with_capacity(2);
    loop {
      let nodes = self.parse_seq(Stop::ExtglobPipeOrClose)?;
      branches.push(nodes);
      match self.peek() {
        Some(b'|') => {
          self.pos += 1;
          continue;
        }
        Some(b')') => {
          self.pos += 1;
          break;
        }
        _ => {
          return Err(ParseError {
            message: "unterminated extglob",
            position: self.pos,
          });
        }
      }
    }
    Ok(branches)
  }
}

fn find_seq(haystack: &[u8], needle: &[u8]) -> Option<usize> {
  memchr::memmem::find(haystack, needle)
}

fn brace_balanced(s: &[u8]) -> bool {
  debug_assert_eq!(s.first().copied(), Some(b'{'));
  let mut depth: i32 = 0;
  let mut i = 0;
  let mut in_class = false;
  let mut has_comma_or_dotdot = false;
  while i < s.len() {
    match s[i] {
      b'\\' => i += 1,
      b'[' if !in_class => in_class = true,
      b']' if in_class => in_class = false,
      b'{' if !in_class => depth += 1,
      b'}' if !in_class => {
        depth -= 1;
        if depth == 0 {
          return has_comma_or_dotdot;
        }
      }
      b',' if !in_class && depth == 1 => has_comma_or_dotdot = true,
      _ => {}
    }
    i += 1;
  }
  false
}

fn contains_slash(nodes: &[Node]) -> bool {
  for n in nodes {
    match n {
      Node::Sep => return true,
      Node::Brace(branches) | Node::Extglob { branches, .. } => {
        for b in branches {
          if contains_slash(b) {
            return true;
          }
        }
      }
      _ => {}
    }
  }
  false
}
