use core::{
    cmp::Ordering,
    fmt::{self, Debug, Display},
};

/// Maximum modulus length (in U256 limbs) supported by the no-alloc modexp path.
///
/// A modulus of `MAX_MODEXP_LEN_M` limbs corresponds to a 1024-byte modulus, which covers EIP-2565
/// realistic inputs with margin. Inputs exceeding this bound must be rejected by the caller.
pub const MAX_MODEXP_LEN_M: usize = 32;

/// Maximum number of exponent bits supported by the no-alloc modexp path.
///
/// Sized as `MAX_MODEXP_LEN_M * 4 * 64` to cover the worst-case bit decomposition of a maximally
/// sized exponent (one bit per u64 entry, as produced by `fcall_bin_decomp`).
pub const MAX_MODEXP_EXP_BITS: usize = MAX_MODEXP_LEN_M * 4 * 64;

/// A 256-bit unsigned integer stored as four little-endian 64-bit limbs.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct U256([u64; 4]); // little-endian: 4 × 64 = 256 bits

impl U256 {
    pub const ZERO: Self = U256([0, 0, 0, 0]);
    pub const ONE: Self = U256([1, 0, 0, 0]);
    pub const TWO: Self = U256([2, 0, 0, 0]);
    pub const MAX: Self = U256([u64::MAX, u64::MAX, u64::MAX, u64::MAX]);

    #[inline(always)]
    pub const fn from_u64s(a: &[u64; 4]) -> Self {
        U256(*a)
    }

    #[inline(always)]
    pub const fn from_u64(a: u64) -> Self {
        U256([a, 0, 0, 0])
    }

    #[inline(always)]
    pub fn as_limbs(&self) -> &[u64; 4] {
        &self.0
    }

    #[inline(always)]
    pub fn as_limbs_mut(&mut self) -> &mut [u64; 4] {
        &mut self.0
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        self.0[0] == 0 && self.0[1] == 0 && self.0[2] == 0 && self.0[3] == 0
    }

    #[inline]
    pub fn is_one(&self) -> bool {
        self.0[0] == 1 && self.0[1] == 0 && self.0[2] == 0 && self.0[3] == 0
    }

    #[inline]
    pub fn lt(&self, other: &Self) -> bool {
        for i in (0..4).rev() {
            if self.0[i] != other.0[i] {
                return self.0[i] < other.0[i];
            }
        }
        false
    }

    #[inline]
    pub fn gt(&self, other: &Self) -> bool {
        for i in (0..4).rev() {
            if self.0[i] != other.0[i] {
                return self.0[i] > other.0[i];
            }
        }
        false
    }

    #[inline]
    pub fn compare(&self, other: &Self) -> Ordering {
        for i in (0..4).rev() {
            if self.0[i] < other.0[i] {
                return Ordering::Less;
            } else if self.0[i] > other.0[i] {
                return Ordering::Greater;
            }
        }
        Ordering::Equal
    }

    pub fn eq_slices(a: &[Self], b: &[Self]) -> bool {
        // TODO: Do with hint and instructions?

        let len_a = a.len();
        let len_b = b.len();
        if len_a != len_b {
            return false;
        }

        for i in 0..len_a {
            if !a[i].eq(&b[i]) {
                return false;
            }
        }

        true
    }

    pub fn lt_slices(a: &[Self], b: &[Self]) -> bool {
        // TODO: Do with hint and instructions?

        let len_a = a.len();
        let len_b = b.len();
        if len_a != len_b {
            return len_a < len_b;
        }

        for i in (0..len_a).rev() {
            if !a[i].eq(&b[i]) {
                return a[i].lt(&b[i]);
            }
        }

        false
    }

    pub fn compare_slices(a: &[U256], b: &[U256]) -> Ordering {
        // TODO: Do with hint and instructions?

        let len_a = a.len();
        let len_b = b.len();

        if len_a != len_b {
            return len_a.cmp(&len_b);
        }

        for i in (0..len_a).rev() {
            match a[i].compare(&b[i]) {
                Ordering::Equal => continue,
                other => return other,
            }
        }

        Ordering::Equal
    }

    #[inline(always)]
    pub fn slice_to_flat(slice: &[U256]) -> &[u64] {
        // Safe because U256 is #[repr(transparent)] over [u64; 4]
        unsafe { core::slice::from_raw_parts(slice.as_ptr() as *const u64, slice.len() * 4) }
    }

    #[inline(always)]
    pub fn flat_to_slice(flat: &[u64]) -> &[U256] {
        debug_assert_eq!(flat.len() % 4, 0, "Flat slice length must be multiple of 4");
        // Safe because U256 is #[repr(transparent)] over [u64; 4]
        unsafe { core::slice::from_raw_parts(flat.as_ptr() as *const U256, flat.len() / 4) }
    }
}

impl Debug for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:016x}{:016x}{:016x}{:016x}", self.0[3], self.0[2], self.0[1], self.0[0])
    }
}

impl Display for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:016x}{:016x}{:016x}{:016x}", self.0[3], self.0[2], self.0[1], self.0[0])
    }
}

impl PartialEq for U256 {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0[3] == other.0[3]
            && self.0[2] == other.0[2]
            && self.0[1] == other.0[1]
            && self.0[0] == other.0[0]
    }
}

/// Scratch space for short-divisor division verification (`rem_short`).
pub struct ShortScratch {
    pub quo: [u64; 8],    // quotient
    pub rem: [u64; 4],    // remainder
    pub q_b: [U256; 2],   // q * b
    pub q_b_r: [U256; 2], // q * b + r
}

impl ShortScratch {
    #[inline(always)]
    pub fn new() -> Self {
        Self { quo: [0u64; 8], rem: [0u64; 4], q_b: [U256::ZERO; 2], q_b_r: [U256::ZERO; 2] }
    }
}

impl Default for ShortScratch {
    fn default() -> Self {
        Self::new()
    }
}

/// Scratch space for the remainder step of long-divisor division verification.
///
/// Each field borrows a caller-provided buffer:
/// - `quo` (>= `2 * len_m * 4` u64s) — hinted quotient flat limbs
/// - `rem` (>= `len_m * 4` u64s) — hinted remainder flat limbs
/// - `q_b` (>= `2 * len_m` U256s) — intermediate `q * b`
/// - `q_b_r` (>= `2 * len_m` U256s) — intermediate `q * b + r`
pub struct RemLongScratch<'a> {
    pub quo: &'a mut [u64],
    pub rem: &'a mut [u64],
    pub q_b: &'a mut [U256],
    pub q_b_r: &'a mut [U256],
}

impl<'a> RemLongScratch<'a> {
    pub fn from_buffers(
        quo: &'a mut [u64],
        rem: &'a mut [u64],
        q_b: &'a mut [U256],
        q_b_r: &'a mut [U256],
    ) -> Self {
        Self { quo, rem, q_b, q_b_r }
    }
}

/// Combined scratch space for long-divisor multiplication-then-reduction (`mul_and_reduce_long`,
/// `square_and_reduce_long`).
///
/// `mul` is the intermediate output buffer for `mul_long` / `square_long` (size >= `2 * len_m`).
/// `rem` is the scratch consumed by `rem_long`.
pub struct LongScratch<'a> {
    pub rem: RemLongScratch<'a>,
    pub mul: &'a mut [U256],
}

impl<'a> LongScratch<'a> {
    pub fn from_buffers(rem: RemLongScratch<'a>, mul: &'a mut [U256]) -> Self {
        Self { rem, mul }
    }
}

/// Umbrella scratch for `modexp`.
///
/// Fields:
/// - `long` — multiplication+reduction scratch (mul intermediate + RemLongScratch)
/// - `bits` (>= `64 * exp_len` u64s) — destination for `fcall_bin_decomp`
/// - `rec_exp` (>= `exp_len` u64s) — recomposed exponent for verification
/// - `base_buf` (>= `len_m` U256s) — persistent reduced base across iterations
/// - `tmp_buf` (>= `len_m` U256s) — second ping-pong buffer alongside the caller's `out`
pub struct ModexpScratch<'a> {
    pub long: LongScratch<'a>,
    pub bits: &'a mut [u64],
    pub rec_exp: &'a mut [u64],
    pub base_buf: &'a mut [U256],
    pub tmp_buf: &'a mut [U256],
}
