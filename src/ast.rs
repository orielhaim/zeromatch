use crate::util::ByteClass;

#[derive(Clone, Debug)]
pub enum Node {
  Literal(Vec<u8>),
  Sep,
  AnyChar,
  Star,
  Globstar,
  Class(ByteClass),
  Brace(Vec<Vec<Node>>),
  Extglob {
    kind: ExtKind,
    branches: Vec<Vec<Node>>,
  },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtKind {
  Negate,
  Optional,
  Plus,
  Star,
  At,
}

#[derive(Clone, Debug)]
pub struct Pattern {
  pub nodes: Vec<Node>,
  pub negated: bool,
  pub has_slash: bool,
}
