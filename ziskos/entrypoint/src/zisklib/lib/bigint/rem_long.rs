use core::cmp::Ordering;

#[cfg(all(feature = "hints", not(all(target_os = "zkvm", target_vendor = "zisk"))))]
use crate::alloc_extern::vec::Vec;

use crate::zisklib::fcall_bigint_div;

use super::{add_agtb, mul_long, RemLongScratch, U256};

/// Computes the remainder of two large numbers (initial call), writing the result into `out`.
///
/// # Assumptions
/// - `len(a) > 0` and `len(b) > 0`
/// - `a` and `b` have no leading zeros (unless `a` being zero)
/// - `b > 0`
/// - `out.len() >= max(len(a), len(b))`
/// - `scratch.quo.len() >= len(a) * 4`, `scratch.rem.len() >= len(b) * 4`,
///   `scratch.q_b.len() >= len(a) + 1`, `scratch.q_b_r.len() >= len(a) + 1`
///
/// # Returns
/// The number of `U256` limbs written to `out` representing `a mod b`.
///
/// # Note
/// Use this for the first reduction when `a` can be arbitrarily large.
/// For subsequent reductions in a loop, use `rem_long` with scratch space.
pub fn rem_long_init(
    a: &[U256],
    b: &[U256],
    scratch: &mut RemLongScratch<'_>,
    out: &mut [U256],
    #[cfg(feature = "hints")] hints: &mut Vec<u64>,
) -> usize {
    let len_a = a.len();
    let len_b = b.len();
    #[cfg(debug_assertions)]
    {
        assert_ne!(len_a, 0, "Input 'a' must have at least one limb");
        assert_ne!(len_b, 0, "Input 'b' must have at least one limb");
        assert!(!b[len_b - 1].is_zero(), "Input 'b' must not have leading zeros");
        if len_a > 1 {
            assert!(!a[len_a - 1].is_zero(), "Input 'a' must not have leading zeros");
        }
    }

    // Check if a = b, a < b or a > b
    let comp = U256::compare_slices(a, b);
    if comp == Ordering::Less {
        out[..len_a].copy_from_slice(a);
        return len_a;
    } else if comp == Ordering::Equal {
        out[0] = U256::ZERO;
        return 1;
    }
    // We can assume a > b from here on

    // Strategy: Hint the division result and then verify it satisfies Euclid's division lemma
    let a_flat = U256::slice_to_flat(a);
    let b_flat = U256::slice_to_flat(b);

    let quo_len_u64 = len_a * 4;
    let rem_len_u64 = len_b * 4;
    let q_b_cap = len_a + 1; // mul_long and add_agtb need this much
    debug_assert!(scratch.quo.len() >= quo_len_u64);
    debug_assert!(scratch.rem.len() >= rem_len_u64);
    debug_assert!(scratch.q_b.len() >= q_b_cap);
    debug_assert!(scratch.q_b_r.len() >= q_b_cap);

    // Hint the quotient and remainder
    let (limbs_quo, limbs_rem) = fcall_bigint_div(
        a_flat,
        b_flat,
        &mut scratch.quo[..quo_len_u64],
        &mut scratch.rem[..rem_len_u64],
        #[cfg(feature = "hints")]
        hints,
    );
    let quo = U256::flat_to_slice(&scratch.quo[..limbs_quo]);
    let rem = U256::flat_to_slice(&scratch.rem[..limbs_rem]);

    // Verify the division
    verify_division(
        a,
        b,
        quo,
        rem,
        &mut scratch.q_b[..q_b_cap],
        &mut scratch.q_b_r[..q_b_cap],
        #[cfg(feature = "hints")]
        hints,
    );

    let rem_len = rem.len();
    out[..rem_len].copy_from_slice(rem);
    rem_len
}

/// Computes the remainder of two large numbers (with scratch), writing the result into `out`.
///
/// # Assumptions
/// - `len(a) > 0` and `len(b) > 0`
/// - `a` and `b` have no leading zeros (unless `a` being zero)
/// - `b > 0`
/// - `out.len() >= max(len(a), len(b))`
///
/// # Returns
/// The number of `U256` limbs written to `out` representing `a mod b`.
///
/// # Note
/// Not optimal for `len(b) == 1`, use `rem_short` instead
pub fn rem_long(
    a: &[U256],
    b: &[U256],
    scratch: &mut RemLongScratch<'_>,
    out: &mut [U256],
    #[cfg(feature = "hints")] hints: &mut Vec<u64>,
) -> usize {
    let len_a = a.len();
    #[cfg(debug_assertions)]
    {
        let len_b = b.len();
        assert_ne!(len_a, 0, "Input 'a' must have at least one limb");
        assert_ne!(len_b, 0, "Input 'b' must have at least one limb");
        assert!(!b[len_b - 1].is_zero(), "Input 'b' must not have leading zeros");
        if len_a > 1 {
            assert!(!a[len_a - 1].is_zero(), "Input 'a' must not have leading zeros");
        }
    }

    // Check if a = b, a < b or a > b
    let comp = U256::compare_slices(a, b);
    if comp == Ordering::Less {
        out[..len_a].copy_from_slice(a);
        return len_a;
    } else if comp == Ordering::Equal {
        out[0] = U256::ZERO;
        return 1;
    }
    // We can assume a > b from here on

    // Strategy: Hint the division result and then verify it satisfies Euclid's division lemma
    let a_flat = U256::slice_to_flat(a);
    let b_flat = U256::slice_to_flat(b);

    // Hint the quotient and remainder
    let (limbs_quo, limbs_rem) = fcall_bigint_div(
        a_flat,
        b_flat,
        scratch.quo,
        scratch.rem,
        #[cfg(feature = "hints")]
        hints,
    );
    let quo = U256::flat_to_slice(&scratch.quo[..limbs_quo]);
    let rem = U256::flat_to_slice(&scratch.rem[..limbs_rem]);

    // Verify the division
    verify_division(
        a,
        b,
        quo,
        rem,
        scratch.q_b,
        scratch.q_b_r,
        #[cfg(feature = "hints")]
        hints,
    );

    let rem_len = rem.len();
    out[..rem_len].copy_from_slice(rem);
    rem_len
}

/// Verify that a = q·b + r
#[inline(always)]
fn verify_division(
    a: &[U256],
    b: &[U256],
    quo: &[U256],
    rem: &[U256],
    q_b: &mut [U256],
    q_b_r: &mut [U256],
    #[cfg(feature = "hints")] hints: &mut Vec<u64>,
) {
    let len_a = a.len();
    let len_b = b.len();
    let len_quo = quo.len();
    let len_rem = rem.len();

    // Since len(a) >= len(b), the division a = q·b + r must satisfy:
    //      1] max{len(q·b), len(r)} <= len(a) => len(q) + len(b) - 1 <= len(q·b) <= len(a)
    //                                         =>                        len(r)   <= len(a)
    //      2] 1 <= len(r) <= len(b)

    // Check 1 <= len(q) <= len(a) - len(b) + 1
    assert!(len_quo > 0, "Quotient must have at least one limb");
    assert!(
        len_quo <= len_a - len_b + 1,
        "Quotient length must be less than or equal to dividend length"
    );
    assert!(!quo[len_quo - 1].is_zero(), "Quotient must not have leading zeros");

    // Multiply the quotient by b
    let q_b_len = mul_long(
        quo,
        b,
        q_b,
        #[cfg(feature = "hints")]
        hints,
    );

    // Check 1 <= len(r)
    assert!(len_rem > 0, "Remainder must have at least one limb");

    if rem[len_rem - 1].is_zero() {
        // If the remainder is zero, then a must be equal to q·b
        assert!(U256::eq_slices(a, &q_b[..q_b_len]), "Remainder is zero, but a != q·b");
    } else {
        // If the remainder is non-zero, then we should check that a must be equal to q·b + r and r < b

        assert!(U256::lt_slices(rem, b), "Remainder must be less than divisor");

        let q_b_r_len = add_agtb(
            &q_b[..q_b_len],
            rem,
            q_b_r,
            #[cfg(feature = "hints")]
            hints,
        );
        assert!(U256::eq_slices(a, &q_b_r[..q_b_r_len]), "a != q·b + r");
    }
}
