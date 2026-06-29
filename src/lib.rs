#![deny(clippy::all)]
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::needless_return)]

mod ast;
mod compile;
mod matcher;
mod options;
mod parse;
mod regex_emit;
mod scan_glob;
mod set;
mod util;

pub use compile::{CompiledGlob, Program};
pub use options::{MatchOptions, Mode};
pub use set::GlobSetMatcher;

mod ffi;
