use crate::ast::{ExtKind, Node};
use crate::options::{MatchOptions, Mode};
use crate::parse::{ParseError, parse};

pub fn make_re_source(pattern: &str, opts: &MatchOptions) -> Result<String, ParseError> {
  let ast = parse(pattern, opts)?;
  let mut s = String::with_capacity(pattern.len() * 2 + 8);
  if !opts.contains {
    s.push('^');
  }
  if ast.negated {
    s.push_str("(?!");
  }
  emit_seq(&ast.nodes, &mut s, opts);
  if ast.negated {
    s.push_str(").*");
  }
  if !opts.contains {
    s.push('$');
  }
  Ok(s)
}

fn emit_seq(nodes: &[Node], out: &mut String, opts: &MatchOptions) {
  for n in nodes {
    emit_node(n, out, opts);
  }
}

fn emit_node(n: &Node, out: &mut String, opts: &MatchOptions) {
  match n {
    Node::Literal(b) => {
      for &c in b {
        if matches!(
          c,
          b'.'
            | b'^'
            | b'$'
            | b'+'
            | b'*'
            | b'?'
            | b'('
            | b')'
            | b'['
            | b']'
            | b'{'
            | b'}'
            | b'|'
            | b'\\'
        ) {
          out.push('\\');
        }
        out.push(c as char);
      }
    }
    Node::Sep => {
      if opts.mode == Mode::Windows {
        out.push_str("[/\\\\]");
      } else {
        out.push('/');
      }
    }
    Node::AnyChar => {
      if opts.mode == Mode::Windows {
        out.push_str("[^/\\\\]");
      } else {
        out.push_str("[^/]");
      }
    }
    Node::Star => {
      if opts.mode == Mode::Windows {
        out.push_str("[^/\\\\]*");
      } else {
        out.push_str("[^/]*");
      }
    }
    Node::Globstar => out.push_str(".*"),
    Node::Class(_) => out.push_str("[^/]"),
    Node::Brace(branches) => {
      out.push_str("(?:");
      for (i, b) in branches.iter().enumerate() {
        if i > 0 {
          out.push('|');
        }
        emit_seq(b, out, opts);
      }
      out.push(')');
    }
    Node::Extglob { kind, branches } => {
      out.push_str("(?:");
      for (i, b) in branches.iter().enumerate() {
        if i > 0 {
          out.push('|');
        }
        emit_seq(b, out, opts);
      }
      out.push(')');
      match kind {
        ExtKind::Optional => out.push('?'),
        ExtKind::Plus => out.push('+'),
        ExtKind::Star => out.push('*'),
        ExtKind::At | ExtKind::Negate => {}
      }
    }
  }
}
