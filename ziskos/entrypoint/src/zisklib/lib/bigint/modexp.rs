// TODO: It can be speed up by using Montgomery multiplication but knowning that divisions are "free"
// For ref: https://www.microsoft.com/en-us/research/wp-content/uploads/1996/01/j37acmon.pdf

#[cfg(all(target_os = "zkvm", target_vendor = "zisk"))]
use crate::alloc_extern::vec;
#[cfg(all(target_os = "zkvm", target_vendor = "zisk"))]
use crate::alloc_extern::vec::Vec;

use crate::zisklib::fcall_bin_decomp;

use super::{
    mul_and_reduce_long, mul_and_reduce_short, rem_long_init, rem_short_init,
    square_and_reduce_long, square_and_reduce_short, LongScratch, ModexpScratch, RemLongScratch,
    ShortScratch, MAX_MODEXP_EXP_BITS, MAX_MODEXP_LEN_M, U256,
};

/// Modular exponentiation of three large numbers
///
/// It assumes that modulus > 0 and len(base),len(exp),len(modulus) > 0
pub fn modexp(
    base: &[U256],
    exp: &[u64],
    modulus: &[U256],
    scratch: &mut ModexpScratch<'_>,
    out: &mut [U256],
    #[cfg(feature = "hints")] hints: &mut Vec<u64>,
) -> usize {
    let len_b = base.len();
    let len_e = exp.len();
    let len_m = modulus.len();
    #[cfg(debug_assertions)]
    {
        assert_ne!(len_b, 0, "Base must have at least one limb");
        assert_ne!(len_e, 0, "Exponent must have at least one limb");
        assert_ne!(len_m, 0, "Modulus must have at least one limb");

        if len_b > 1 {
            assert!(!base[len_b - 1].is_zero(), "Base must not have leading zeros");
        }
        if len_e > 1 {
            assert_ne!(exp.last().unwrap(), &0, "Exponent must not have leading zeros");
        }
        if len_m > 1 {
            assert!(!modulus[len_m - 1].is_zero(), "Modulus must not have leading zeros");
        } else {
            assert!(!modulus[0].is_zero(), "Modulus must not be zero");
        }
    }

    // If modulus == 0, return zeros
    if len_m == 1 && modulus[0].is_zero() {
        out[0] = U256::ZERO;
        return 1;
    }

    // If modulus == 1, then base^exp (mod 1) is always 0
    if len_m == 1 && modulus[0].is_one() {
        out[0] = U256::ZERO;
        return 1;
    }

    // If exp == 0, then base^0 (mod modulus) is 1
    if len_e == 1 && exp[0] == 0 {
        out[0] = U256::ONE;
        return 1;
    }

    if len_b == 1 {
        // If base == 0, then 0^exp (mod modulus) is 0
        if base[0].is_zero() {
            out[0] = U256::ZERO;
            return 1;
        }

        // If base == 1, then 1^exp (mod modulus) is 1
        if base[0].is_one() {
            out[0] = U256::ONE;
            return 1;
        }
    }

    // We can assume from now on that base,modulus > 1 and exp > 0
    if len_m == 1 {
        modexp_short(
            base,
            exp,
            &modulus[0],
            scratch,
            out,
            #[cfg(feature = "hints")]
            hints,
        )
    } else {
        modexp_long(
            base,
            exp,
            modulus,
            scratch,
            out,
            #[cfg(feature = "hints")]
            hints,
        )
    }
}

/// Short modexp when modulus fits in a single U256
fn modexp_short(
    base: &[U256],
    exp: &[u64],
    modulus: &U256,
    scratch: &mut ModexpScratch<'_>,
    out: &mut [U256],
    #[cfg(feature = "hints")] hints: &mut Vec<u64>,
) -> usize {
    let len_e = exp.len();

    // Compute base = base (mod modulus)
    let base = rem_short_init(
        base,
        modulus,
        &mut scratch.long.rem,
        #[cfg(feature = "hints")]
        hints,
    );

    // Hint exponent bits
    let len = fcall_bin_decomp(
        exp,
        scratch.bits,
        #[cfg(feature = "hints")]
        hints,
    );
    assert!(len > 0 && scratch.bits[0] == 1, "Exponent must be non-zero");

    // We should recompose the exponent from bits to verify correctness
    debug_assert!(scratch.rec_exp.len() >= len_e);
    let rec_exp = &mut scratch.rec_exp[..len_e];
    rec_exp.fill(0);

    // Recompose the MSB
    let bits_pos = len - 1;
    let limb_idx = bits_pos / 64;
    let bit_in_limb = bits_pos % 64;
    rec_exp[limb_idx] = 1u64 << bit_in_limb;

    // Scratch space
    let mut short_scratch = ShortScratch::new();

    // Initialize out_val = base
    let mut out_val = base;
    for bit_idx in 1..len {
        let bit = scratch.bits[bit_idx];
        if out_val.is_zero() {
            out[0] = U256::ZERO;
            return 1;
        }

        // Compute out_val = out_val^2 (mod modulus)
        out_val = square_and_reduce_short(
            &out_val,
            modulus,
            &mut short_scratch,
            #[cfg(feature = "hints")]
            hints,
        );

        if bit == 1 {
            // Compute out_val = (out_val * base) (mod modulus)
            out_val = mul_and_reduce_short(
                &out_val,
                &base,
                modulus,
                &mut short_scratch,
                #[cfg(feature = "hints")]
                hints,
            );

            // Recompose the exponent
            let bits_pos = len - 1 - bit_idx;
            let limb_idx = bits_pos / 64;
            let bit_in_limb = bits_pos % 64;
            rec_exp[limb_idx] |= 1u64 << bit_in_limb;
        }
    }

    assert_eq!(rec_exp[..], *exp, "Exponent decomposition mismatch");

    out[0] = out_val;
    1
}

/// Long modexp when modulus requires multiple U256 limbs
fn modexp_long(
    base: &[U256],
    exp: &[u64],
    modulus: &[U256],
    scratch: &mut ModexpScratch<'_>,
    out: &mut [U256],
    #[cfg(feature = "hints")] hints: &mut Vec<u64>,
) -> usize {
    let len_e = exp.len();
    let len_m = modulus.len();

    // Compute base = base (mod modulus)
    let base_len = rem_long_init(
        base,
        modulus,
        &mut scratch.long.rem,
        &mut scratch.base_buf[..],
        #[cfg(feature = "hints")]
        hints,
    );

    // Hint exponent bits
    let len = fcall_bin_decomp(
        exp,
        scratch.bits,
        #[cfg(feature = "hints")]
        hints,
    );
    assert!(len > 0 && scratch.bits[0] == 1, "Exponent must be non-zero");

    // We should recompose the exponent from bits to verify correctness
    debug_assert!(scratch.rec_exp.len() >= len_e);
    let rec_exp = &mut scratch.rec_exp[..len_e];
    rec_exp.fill(0);

    // Recompose the MSB
    let bits_pos = len - 1;
    let limb_idx = bits_pos / 64;
    let bit_in_limb = bits_pos % 64;
    rec_exp[limb_idx] = 1u64 << bit_in_limb;

    // Initialize out = base
    out[..base_len].copy_from_slice(&scratch.base_buf[..base_len]);
    let mut cur_in_out = true; // tracks which buffer holds the current value
    let mut cur_len = base_len;

    for bit_idx in 1..len {
        let bit = scratch.bits[bit_idx];

        // Early-out: if current is zero, result is zero.
        let cur_first_is_zero = if cur_in_out {
            cur_len == 1 && out[0].is_zero()
        } else {
            cur_len == 1 && scratch.tmp_buf[0].is_zero()
        };
        if cur_first_is_zero {
            out[0] = U256::ZERO;
            return 1;
        }

        // Compute cur = cur^2 (mod modulus) into the OTHER buffer.
        let new_len = if cur_in_out {
            square_and_reduce_long(
                &out[..cur_len],
                modulus,
                &mut scratch.long,
                &mut scratch.tmp_buf[..len_m],
                #[cfg(feature = "hints")]
                hints,
            )
        } else {
            square_and_reduce_long(
                &scratch.tmp_buf[..cur_len],
                modulus,
                &mut scratch.long,
                &mut out[..len_m],
                #[cfg(feature = "hints")]
                hints,
            )
        };
        cur_in_out = !cur_in_out;
        cur_len = new_len;

        if bit == 1 {
            // Compute cur = cur * base (mod modulus) into the OTHER buffer.
            let new_len = if cur_in_out {
                mul_and_reduce_long(
                    &out[..cur_len],
                    &scratch.base_buf[..base_len],
                    modulus,
                    &mut scratch.long,
                    &mut scratch.tmp_buf[..len_m],
                    #[cfg(feature = "hints")]
                    hints,
                )
            } else {
                mul_and_reduce_long(
                    &scratch.tmp_buf[..cur_len],
                    &scratch.base_buf[..base_len],
                    modulus,
                    &mut scratch.long,
                    &mut out[..len_m],
                    #[cfg(feature = "hints")]
                    hints,
                )
            };
            cur_in_out = !cur_in_out;
            cur_len = new_len;

            // Recompose the exponent
            let bits_pos = len - 1 - bit_idx;
            let limb_idx = bits_pos / 64;
            let bit_in_limb = bits_pos % 64;
            rec_exp[limb_idx] |= 1u64 << bit_in_limb;
        }
    }

    assert_eq!(rec_exp[..], *exp, "Exponent decomposition mismatch");

    // Ensure final result lives in `out`.
    if !cur_in_out {
        out[..cur_len].copy_from_slice(&scratch.tmp_buf[..cur_len]);
    }
    cur_len
}

/// Compute modular exponentiation from big-endian byte arrays
///
/// ### Safety
///
/// The caller must ensure that:
/// - `base_ptr` points to an array of `base_len` bytes (big-endian)
/// - `exp_ptr` points to an array of `exp_len` bytes (big-endian)
/// - `modulus_ptr` points to an array of `modulus_len` bytes (big-endian)
/// - `result_ptr` points to an array of at least `modulus_len` bytes
///
/// Returns the number of bytes written to `result_ptr` (always equals `modulus_len`, zero-padded)
#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
#[inline]
pub(crate) unsafe fn modexp_bytes_c(
    base_ptr: *const u8,
    base_len: usize,
    exp_ptr: *const u8,
    exp_len: usize,
    modulus_ptr: *const u8,
    modulus_len: usize,
    result_ptr: *mut u8,
    #[cfg(feature = "hints")] hints: &mut Vec<u64>,
) -> usize {
    // Loud bound check on input byte lengths.
    if base_len > MAX_MODEXP_LEN_M * 32
        || exp_len > MAX_MODEXP_LEN_M * 32
        || modulus_len > MAX_MODEXP_LEN_M * 32
    {
        core::intrinsics::abort();
    }

    let base_bytes = core::slice::from_raw_parts(base_ptr, base_len);
    let exp_bytes = core::slice::from_raw_parts(exp_ptr, exp_len);
    let modulus_bytes = core::slice::from_raw_parts(modulus_ptr, modulus_len);

    // Convert from big-endian bytes to little-endian u64/U256 arrays
    let mut base_u256 = [U256::ZERO; MAX_MODEXP_LEN_M];
    let mut modulus_u256 = [U256::ZERO; MAX_MODEXP_LEN_M];
    let mut exp_u64 = [0u64; MAX_MODEXP_LEN_M * 4];

    let base_n = bytes_be_to_u256_le_into(base_bytes, &mut base_u256);
    let exp_n = bytes_be_to_u64_le_into(exp_bytes, &mut exp_u64);
    let mod_n = bytes_be_to_u256_le_into(modulus_bytes, &mut modulus_u256);

    let mut quo_buf = [0u64; 2 * MAX_MODEXP_LEN_M * 4];
    let mut rem_buf = [0u64; MAX_MODEXP_LEN_M * 4];
    // +1 because rem_long slices q_b[..mul_len + 1] where mul_len can reach 2 * len_m.
    let mut q_b_buf = [U256::ZERO; 2 * MAX_MODEXP_LEN_M + 1];
    let mut q_b_r_buf = [U256::ZERO; 2 * MAX_MODEXP_LEN_M + 1];
    let mut mul_buf = [U256::ZERO; 2 * MAX_MODEXP_LEN_M];
    let mut bits_buf = [0u64; MAX_MODEXP_EXP_BITS];
    let mut rec_exp_buf = [0u64; MAX_MODEXP_LEN_M * 4];
    let mut base_buf = [U256::ZERO; MAX_MODEXP_LEN_M];
    let mut tmp_buf = [U256::ZERO; MAX_MODEXP_LEN_M];
    let mut out_u256 = [U256::ZERO; MAX_MODEXP_LEN_M];

    let rem_scratch =
        RemLongScratch::from_buffers(&mut quo_buf, &mut rem_buf, &mut q_b_buf, &mut q_b_r_buf);
    let long_scratch = LongScratch::from_buffers(rem_scratch, &mut mul_buf);
    let mut scratch = ModexpScratch {
        long: long_scratch,
        bits: &mut bits_buf,
        rec_exp: &mut rec_exp_buf,
        base_buf: &mut base_buf,
        tmp_buf: &mut tmp_buf,
    };

    let out_n = modexp(
        &base_u256[..base_n],
        &exp_u64[..exp_n],
        &modulus_u256[..mod_n],
        &mut scratch,
        &mut out_u256,
        #[cfg(feature = "hints")]
        hints,
    );

    // Convert result back to big-endian bytes with proper length
    let result = core::slice::from_raw_parts_mut(result_ptr, modulus_len);
    u256_le_to_bytes_be(&out_u256[..out_n], result);

    modulus_len
}

/// Convert big-endian bytes to little-endian u64 array, writing into `out`. Returns the number of
/// u64 limbs written. Empty input yields a single zero limb.
#[allow(dead_code)]
fn bytes_be_to_u64_le_into(bytes: &[u8], out: &mut [u64]) -> usize {
    if bytes.is_empty() {
        out[0] = 0;
        return 1;
    }

    // Skip leading zeros but keep at least one limb
    let first_nonzero = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len() - 1);
    let bytes = &bytes[first_nonzero..];

    if bytes.is_empty() {
        out[0] = 0;
        return 1;
    }

    // Process bytes into u64 limbs
    let num_limbs = bytes.len().div_ceil(8);
    debug_assert!(out.len() >= num_limbs, "bytes_be_to_u64_le_into: out too small");
    out[..num_limbs].fill(0);
    for (i, &byte) in bytes.iter().rev().enumerate() {
        let limb_idx = i / 8;
        let byte_idx = i % 8;
        out[limb_idx] |= (byte as u64) << (byte_idx * 8);
    }

    num_limbs
}

/// Convert big-endian bytes to little-endian U256 array, writing into `out`. Returns the number of
/// U256 limbs written. The trailing high limb is zero-padded to fill a full U256 as the existing
/// behavior requires.
#[allow(dead_code)]
fn bytes_be_to_u256_le_into(bytes: &[u8], out: &mut [U256]) -> usize {
    // Decompose to u64 limbs into a stack scratch, then pack into U256 groups of 4.
    let mut u64_scratch = [0u64; MAX_MODEXP_LEN_M * 4];
    let n_u64 = bytes_be_to_u64_le_into(bytes, &mut u64_scratch);

    let padded_len = n_u64.next_multiple_of(4);
    debug_assert!(padded_len <= u64_scratch.len(), "bytes_be_to_u256_le_into: too many u64 limbs");
    // Zero-pad already provided by u64_scratch initialisation.

    let n_u256 = padded_len / 4;
    debug_assert!(out.len() >= n_u256, "bytes_be_to_u256_le_into: out too small");
    for i in 0..n_u256 {
        let base = i * 4;
        out[i] = U256::from_u64s(&[
            u64_scratch[base],
            u64_scratch[base + 1],
            u64_scratch[base + 2],
            u64_scratch[base + 3],
        ]);
    }

    n_u256
}

/// Convert little-endian U256 array to big-endian bytes
#[allow(dead_code)]
fn u256_le_to_bytes_be(limbs: &[U256], output: &mut [u8]) {
    let flat = U256::slice_to_flat(limbs);
    let out_len = output.len();
    output.fill(0);

    for (i, &limb) in flat.iter().enumerate() {
        for j in 0..8 {
            let byte_val = ((limb >> (j * 8)) & 0xFF) as u8;
            let pos_from_end = i * 8 + j;
            if pos_from_end < out_len {
                output[out_len - 1 - pos_from_end] = byte_val;
            }
        }
    }
}
