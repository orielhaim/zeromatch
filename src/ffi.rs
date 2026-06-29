use std::cell::UnsafeCell;
use std::ptr;

use napi::JsString;
use napi::bindgen_prelude::*;
use napi::sys;
use napi_derive::napi;

use crate::compile::CompiledGlob;
use crate::matcher::matches as do_match;
use crate::options::{MatchOptions, Mode};
use crate::regex_emit::make_re_source;
use crate::scan_glob::scan as do_scan;
use crate::set::GlobSetMatcher;

const STACK_BUF_LEN: usize = 1024;

thread_local! {

  static HEAP_SCRATCH: UnsafeCell<Vec<u8>> = const { UnsafeCell::new(Vec::new()) };
}

#[inline]
unsafe fn with_js_bytes_raw<R>(
  env: sys::napi_env,
  val: sys::napi_value,
  f: impl FnOnce(&[u8]) -> R,
) -> Result<R> {
  let mut stack: std::mem::MaybeUninit<[u8; STACK_BUF_LEN]> = std::mem::MaybeUninit::uninit();
  let stack_ptr = stack.as_mut_ptr() as *mut std::os::raw::c_char;
  let mut written: usize = 0;
  let status =
    unsafe { sys::napi_get_value_string_utf8(env, val, stack_ptr, STACK_BUF_LEN, &mut written) };
  if status != sys::Status::napi_ok {
    return Err(Error::from_status(Status::from(status)));
  }

  if written < STACK_BUF_LEN - 1 {
    let slice = unsafe { std::slice::from_raw_parts(stack_ptr as *const u8, written) };
    return Ok(f(slice));
  }

  with_heap_scratch(env, val, f)
}

#[cold]
#[inline(never)]
fn with_heap_scratch<R>(
  env: sys::napi_env,
  val: sys::napi_value,
  f: impl FnOnce(&[u8]) -> R,
) -> Result<R> {
  HEAP_SCRATCH.with(|cell: &UnsafeCell<Vec<u8>>| {
    let buf = unsafe { &mut *cell.get() };

    let mut len: usize = 0;
    let status = unsafe { sys::napi_get_value_string_utf8(env, val, ptr::null_mut(), 0, &mut len) };
    if status != sys::Status::napi_ok {
      return Err(Error::from_status(Status::from(status)));
    }
    let needed = len + 1;
    if buf.capacity() < needed {
      buf.reserve(needed - buf.capacity());
    }
    unsafe {
      buf.set_len(needed);
    }

    let mut written: usize = 0;
    let status = unsafe {
      sys::napi_get_value_string_utf8(
        env,
        val,
        buf.as_mut_ptr() as *mut std::os::raw::c_char,
        needed,
        &mut written,
      )
    };
    if status != sys::Status::napi_ok {
      unsafe {
        buf.set_len(0);
      }
      return Err(Error::from_status(Status::from(status)));
    }
    unsafe {
      buf.set_len(written);
    }
    Ok(f(&buf[..written]))
  })
}

#[inline(always)]
fn with_js_bytes<R>(s: &JsString, f: impl FnOnce(&[u8]) -> R) -> Result<R> {
  let v = napi::JsValue::value(s);
  unsafe { with_js_bytes_raw(v.env, v.value, f) }
}

#[napi(object)]
#[derive(Default)]
pub struct JsMatchOptions {
  pub dot: Option<bool>,
  pub nocase: Option<bool>,
  pub windows: Option<bool>,
  pub contains: Option<bool>,
  #[napi(js_name = "matchBase")]
  pub match_base: Option<bool>,
  pub nobrace: Option<bool>,
  pub nobracket: Option<bool>,
  pub noextglob: Option<bool>,
  pub noglobstar: Option<bool>,
  pub nonegate: Option<bool>,
  #[napi(js_name = "strictSlashes")]
  pub strict_slashes: Option<bool>,
  #[napi(js_name = "maxBraceDepth")]
  pub max_brace_depth: Option<u32>,
  #[napi(js_name = "maxLength")]
  pub max_length: Option<u32>,
}

impl JsMatchOptions {
  fn into_native(self) -> MatchOptions {
    let mut o = MatchOptions::default();
    if let Some(v) = self.dot {
      o.dot = v;
    }
    if let Some(v) = self.nocase {
      o.nocase = v;
    }
    if let Some(v) = self.windows {
      o.mode = if v { Mode::Windows } else { Mode::Posix };
    }
    if let Some(v) = self.contains {
      o.contains = v;
    }
    if let Some(v) = self.match_base {
      o.match_base = v;
    }
    if let Some(v) = self.nobrace {
      o.nobrace = v;
    }
    if let Some(v) = self.nobracket {
      o.nobracket = v;
    }
    if let Some(v) = self.noextglob {
      o.noextglob = v;
    }
    if let Some(v) = self.noglobstar {
      o.noglobstar = v;
    }
    if let Some(v) = self.noglobstar {
      o.noglobstar = v;
    }
    if let Some(v) = self.nonegate {
      o.nonegate = v;
    }
    if let Some(v) = self.strict_slashes {
      o.strict_slashes = v;
    }
    if let Some(v) = self.max_brace_depth {
      o.max_brace_depth = v.min(255) as u8;
    }
    if let Some(v) = self.max_length {
      o.max_length = v as usize;
    }
    o
  }
}

#[inline]
fn opts_or_default(opts: Option<JsMatchOptions>) -> MatchOptions {
  opts.unwrap_or_default().into_native()
}

#[cold]
#[inline(never)]
fn map_err(e: crate::parse::ParseError) -> Error {
  Error::new(
    Status::InvalidArg,
    format!("{} (at byte {})", e.message, e.position),
  )
}

#[napi(js_name = "isMatch")]
#[inline]
pub fn is_match(
  input: JsString,
  pattern: JsString,
  options: Option<JsMatchOptions>,
) -> Result<bool> {
  let opts = opts_or_default(options);
  let pattern_str = with_js_bytes(&pattern, |b| unsafe {
    std::str::from_utf8_unchecked(b).to_owned()
  })?;
  let glob = CompiledGlob::new(&pattern_str, opts).map_err(map_err)?;
  with_js_bytes(&input, |bytes| do_match(&glob.program, &glob.opts, bytes))
}

#[napi(js_name = "isMatchBuffer")]
#[inline]
pub fn is_match_buffer(
  input: &[u8],
  pattern: JsString,
  options: Option<JsMatchOptions>,
) -> Result<bool> {
  let opts = opts_or_default(options);
  let pattern_str = with_js_bytes(&pattern, |b| unsafe {
    std::str::from_utf8_unchecked(b).to_owned()
  })?;
  let glob = CompiledGlob::new(&pattern_str, opts).map_err(map_err)?;
  Ok(do_match(&glob.program, &glob.opts, input))
}

#[napi(js_name = "makeRe")]
pub fn make_re(pattern: String, options: Option<JsMatchOptions>) -> Result<String> {
  let opts = opts_or_default(options);
  make_re_source(&pattern, &opts).map_err(map_err)
}

#[napi(object)]
pub struct JsScanResult {
  pub prefix: String,
  pub base: String,
  pub glob: String,
  #[napi(js_name = "isBrace")]
  pub is_brace: bool,
  #[napi(js_name = "isBracket")]
  pub is_bracket: bool,
  #[napi(js_name = "isGlob")]
  pub is_glob: bool,
  #[napi(js_name = "isExtglob")]
  pub is_extglob: bool,
  #[napi(js_name = "isGlobstar")]
  pub is_globstar: bool,
  pub negated: bool,
  #[napi(js_name = "negatedExtglob")]
  pub negated_extglob: bool,
}

#[napi]
pub fn scan(input: String, options: Option<JsMatchOptions>) -> JsScanResult {
  let opts = opts_or_default(options);
  let r = do_scan(&input, &opts);
  JsScanResult {
    prefix: r.prefix,
    base: r.base,
    glob: r.glob,
    is_brace: r.is_brace,
    is_bracket: r.is_bracket,
    is_glob: r.is_glob,
    is_extglob: r.is_extglob,
    is_globstar: r.is_globstar,
    negated: r.negated,
    negated_extglob: r.negated_extglob,
  }
}

#[napi]
pub struct Glob {
  inner: CompiledGlob,
}

#[napi]
impl Glob {
  #[napi(constructor)]
  pub fn new(pattern: String, options: Option<JsMatchOptions>) -> Result<Self> {
    let opts = opts_or_default(options);
    let inner = CompiledGlob::new(&pattern, opts).map_err(map_err)?;
    Ok(Self { inner })
  }

  #[napi(js_name = "test")]
  #[inline]
  pub fn test(&self, input: JsString) -> bool {
    with_js_bytes(&input, |bytes| {
      do_match(&self.inner.program, &self.inner.opts, bytes)
    })
    .unwrap_or(false)
  }

  #[napi(js_name = "testBuffer")]
  #[inline]
  pub fn test_buffer(&self, input: &[u8]) -> bool {
    do_match(&self.inner.program, &self.inner.opts, input)
  }

  #[napi(js_name = "testMany")]
  pub fn test_many(&self, inputs: Vec<JsString>) -> Vec<bool> {
    inputs
      .iter()
      .map(|s| {
        with_js_bytes(s, |b| do_match(&self.inner.program, &self.inner.opts, b)).unwrap_or(false)
      })
      .collect()
  }

  #[napi(js_name = "filter")]
  pub fn filter(&self, inputs: Vec<String>) -> Vec<String> {
    inputs
      .into_iter()
      .filter(|s| do_match(&self.inner.program, &self.inner.opts, s.as_bytes()))
      .collect()
  }

  #[napi(js_name = "filterIndices")]
  pub fn filter_indices(&self, inputs: Vec<JsString>) -> Vec<u32> {
    let mut out = Vec::new();
    for (i, s) in inputs.iter().enumerate() {
      let m =
        with_js_bytes(s, |b| do_match(&self.inner.program, &self.inner.opts, b)).unwrap_or(false);
      if m {
        out.push(i as u32);
      }
    }
    out
  }

  #[napi(getter)]
  pub fn negated(&self) -> bool {
    self.inner.program.negated
  }
}

#[napi]
pub struct GlobSet {
  inner: GlobSetMatcher,
}

#[napi]
impl GlobSet {
  #[napi(constructor)]
  pub fn new(patterns: Vec<String>, options: Option<JsMatchOptions>) -> Result<Self> {
    let opts = opts_or_default(options);
    let inner = GlobSetMatcher::new(&patterns, opts).map_err(map_err)?;
    Ok(Self { inner })
  }

  #[napi(js_name = "test")]
  #[inline]
  pub fn test(&self, input: JsString) -> bool {
    with_js_bytes(&input, |b| self.inner.is_match(b)).unwrap_or(false)
  }

  #[napi(js_name = "testBuffer")]
  #[inline]
  pub fn test_buffer(&self, input: &[u8]) -> bool {
    self.inner.is_match(input)
  }

  #[napi(js_name = "testMany")]
  pub fn test_many(&self, inputs: Vec<JsString>) -> Vec<bool> {
    inputs
      .iter()
      .map(|s| with_js_bytes(s, |b| self.inner.is_match(b)).unwrap_or(false))
      .collect()
  }

  #[napi(js_name = "matches")]
  pub fn matches(&self, input: JsString) -> Vec<u32> {
    let mut out = Vec::with_capacity(4);
    let _ = with_js_bytes(&input, |b| {
      self.inner.matched_indices(b, &mut out);
    });
    out
  }
}

#[napi_derive::module_init]
fn init() {
  std::panic::set_hook(Box::new(|_info| {}));
}
