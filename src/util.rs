#[derive(Clone, Copy, Debug)]
pub struct ByteClass {
  pub bits: [u64; 4],
}

impl ByteClass {
  pub const EMPTY: Self = Self { bits: [0; 4] };

  #[inline(always)]
  pub fn set(&mut self, b: u8) {
    let i = (b >> 6) as usize;
    let m = 1u64 << (b & 63);
    self.bits[i] |= m;
  }

  #[inline(always)]
  pub fn add_range(&mut self, lo: u8, hi: u8) {
    let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    let mut b = lo;
    loop {
      self.set(b);
      if b == hi {
        break;
      }
      b += 1;
    }
  }

  #[inline(always)]
  pub fn contains(self, b: u8) -> bool {
    let i = (b >> 6) as usize;
    let m = 1u64 << (b & 63);
    // SAFETY: `i < 4` because `b >> 6` ∈ {0,1,2,3}.
    unsafe { (*self.bits.get_unchecked(i) & m) != 0 }
  }

  #[inline]
  pub fn fold_ascii_case(&mut self) {
    let w = self.bits[1];
    let upper_mask: u64 = ((1u64 << 26) - 1) << 1; // 'A'..='Z'
    let lower_mask: u64 = ((1u64 << 26) - 1) << 33; // 'a'..='z'
    let uppers = w & upper_mask;
    let lowers = w & lower_mask;
    let promoted_lowers = uppers << 32;
    let promoted_uppers = lowers >> 32;
    self.bits[1] = w | promoted_lowers | promoted_uppers;
  }

  #[inline]
  pub fn negate(&mut self) {
    for w in &mut self.bits {
      *w = !*w;
    }
  }
}

#[inline(always)]
pub fn ascii_lower(b: u8) -> u8 {
  // Branchless: add 0x20 iff b ∈ A..=Z.
  let is_upper = b.wrapping_sub(b'A') < 26;
  if is_upper {
    b | 0x20
  } else {
    b
  }
}

pub fn posix_class(name: &[u8]) -> Option<ByteClass> {
  let mut cls = ByteClass::EMPTY;
  match name {
    b"alnum" => {
      cls.add_range(b'0', b'9');
      cls.add_range(b'A', b'Z');
      cls.add_range(b'a', b'z');
    }
    b"alpha" => {
      cls.add_range(b'A', b'Z');
      cls.add_range(b'a', b'z');
    }
    b"ascii" => {
      cls.add_range(0, 0x7F);
    }
    b"blank" => {
      cls.set(b' ');
      cls.set(b'\t');
    }
    b"cntrl" => {
      cls.add_range(0, 0x1F);
      cls.set(0x7F);
    }
    b"digit" => {
      cls.add_range(b'0', b'9');
    }
    b"graph" => {
      cls.add_range(0x21, 0x7E);
    }
    b"lower" => {
      cls.add_range(b'a', b'z');
    }
    b"print" => {
      cls.add_range(0x20, 0x7E);
    }
    b"punct" => {
      for &c in b"-!\"#$%&'()*+,./:;<=>?@[\\]^_`{|}~" {
        cls.set(c);
      }
    }
    b"space" => {
      for &c in b" \t\r\n\x0B\x0C" {
        cls.set(c);
      }
    }
    b"upper" => {
      cls.add_range(b'A', b'Z');
    }
    b"word" => {
      cls.add_range(b'0', b'9');
      cls.add_range(b'A', b'Z');
      cls.add_range(b'a', b'z');
      cls.set(b'_');
    }
    b"xdigit" => {
      cls.add_range(b'0', b'9');
      cls.add_range(b'A', b'F');
      cls.add_range(b'a', b'f');
    }
    _ => return None,
  }
  Some(cls)
}

/// Branch-prediction hint for the unhappy path.
#[cold]
#[inline(never)]
pub fn cold_path() {}
