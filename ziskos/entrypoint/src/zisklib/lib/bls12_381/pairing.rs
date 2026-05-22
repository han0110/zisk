//! Pairing over BLS12-381 curve

#[cfg(all(target_os = "zkvm", target_vendor = "zisk"))]
use crate::alloc_extern::vec::Vec;

use crate::zisklib::lib::utils::{eq, is_one, lt};

use super::{
    constants::{G1_IDENTITY, G2_IDENTITY, P, P_MINUS_ONE},
    curve::{
        g1_bytes_be_to_u64_le_bls12_381, is_on_curve_bls12_381, is_on_subgroup_bls12_381,
        neg_bls12_381,
    },
    final_exp::final_exp_bls12_381,
    fp12::mul_fp12_bls12_381,
    miller_loop::{miller_loop_batch_bls12_381, miller_loop_bls12_381},
    twist::{
        g2_bytes_be_to_u64_le_bls12_381, is_on_curve_twist_bls12_381,
        is_on_subgroup_twist_bls12_381,
    },
    PAIRING_BATCH_BLS12,
};

/// Pairing check result codes
#[allow(dead_code)]
pub(crate) const PAIRING_CHECK_SUCCESS: u8 = 0;
#[allow(dead_code)]
pub(crate) const PAIRING_CHECK_FAILED: u8 = 1;
const PAIRING_CHECK_ERR_G1_NOT_IN_FIELD: u8 = 2;
const PAIRING_CHECK_ERR_G1_NOT_ON_CURVE: u8 = 3;
const PAIRING_CHECK_ERR_G1_NOT_IN_SUBGROUP: u8 = 4;
const PAIRING_CHECK_ERR_G2_NOT_IN_FIELD: u8 = 5;
const PAIRING_CHECK_ERR_G2_NOT_ON_CURVE: u8 = 6;
const PAIRING_CHECK_ERR_G2_NOT_IN_SUBGROUP: u8 = 7;

/// Optimal Ate Pairing e: G1 x G2 -> GT over the BLS12-381 curve
/// where G1 = E(Fp)[r] = E(Fp), G2 = E'(Fp2)[r] and GT = μ_r (the r-th roots of unity over Fp12*)
/// the involved curves are E/Fp: y² = x³ + 4 and E'/Fp2: y² = x³ + 4·(1+u)
///  pairingBLS12-381:
///          input: P ∈ G1 and Q ∈ G2
///          output: e(P,Q) ∈ GT
pub fn pairing_bls12_381(
    p: &[u64; 12],
    q: &[u64; 24],
    #[cfg(feature = "hints")] hints: &mut Vec<u64>,
) -> [u64; 72] {
    // e(P, 𝒪) = e(𝒪, Q) = 1;
    if *p == G1_IDENTITY || *q == G2_IDENTITY {
        let mut one = [0; 72];
        one[0] = 1;
        return one;
    }

    // Miller loop
    let miller_loop = miller_loop_bls12_381(
        p,
        q,
        #[cfg(feature = "hints")]
        hints,
    );

    // Final exponentiation
    final_exp_bls12_381(
        &miller_loop,
        #[cfg(feature = "hints")]
        hints,
    )
}

/// Computes the optimal Ate pairing for a batch of G1 and G2 points over the BLS12-381 curve
/// and multiplies the results together:
///     e(P₁, Q₁) · e(P₂, Q₂) · ... · e(Pₙ, Qₙ) ∈ GT
///
/// Each iterator item is a pre-validated pair or a propagated error code; the first `Err` item
/// short-circuits the computation.
pub fn pairing_batch_bls12_381(
    pairs: impl Iterator<Item = Result<([u64; 12], [u64; 24]), u8>>,
    #[cfg(feature = "hints")] hints: &mut Vec<u64>,
) -> Result<[u64; 72], u8> {
    let mut g1_arr = [[0u64; 12]; PAIRING_BATCH_BLS12];
    let mut g2_arr = [[0u64; 24]; PAIRING_BATCH_BLS12];
    let mut batch_len: usize = 0;

    let mut f: Option<[u64; 72]> = None;

    for item in pairs {
        let (g1, g2) = item?;
        g1_arr[batch_len] = g1;
        g2_arr[batch_len] = g2;
        batch_len += 1;

        if batch_len == PAIRING_BATCH_BLS12 {
            let batch_f = miller_loop_batch_bls12_381(
                &g1_arr,
                &g2_arr,
                #[cfg(feature = "hints")]
                hints,
            );
            f = Some(match f {
                Some(f) => mul_fp12_bls12_381(
                    &f,
                    &batch_f,
                    #[cfg(feature = "hints")]
                    hints,
                ),
                None => batch_f,
            });
            batch_len = 0;
        }
    }

    if batch_len > 0 {
        let batch_f = miller_loop_batch_bls12_381(
            &g1_arr[..batch_len],
            &g2_arr[..batch_len],
            #[cfg(feature = "hints")]
            hints,
        );
        f = Some(match f {
            Some(f) => mul_fp12_bls12_381(
                &f,
                &batch_f,
                #[cfg(feature = "hints")]
                hints,
            ),
            None => batch_f,
        });
    }

    let Some(f) = f else {
        // Empty input returns 1
        let mut one = [0; 72];
        one[0] = 1;
        return Ok(one);
    };

    Ok(final_exp_bls12_381(
        &f,
        #[cfg(feature = "hints")]
        hints,
    ))
}

/// BLS12-381 single-pair validation for the pairing check.
///
/// Validates that the points have canonical field elements, are on curve, and in subgroup.
///
/// # Returns
/// * `Ok(true)` - Pair is well-formed and should be included in the pairing product
/// * `Ok(false)` - Either point is the identity; the pair contributes 1 and can be skipped
/// * `Err(code)` - One of the [PAIRING_CHECK_ERR_*] validation error codes
pub fn pairing_check_bls12_381(
    g1: &[u64; 12],
    g2: &[u64; 24],
    #[cfg(feature = "hints")] hints: &mut Vec<u64>,
) -> Result<bool, u8> {
        let g1_is_inf = eq(g1, &G1_IDENTITY);
        let g2_is_inf = eq(g2, &G2_IDENTITY);

        if g1_is_inf && g2_is_inf {
            // If p = 𝒪 and q = 𝒪 => e(𝒪, 𝒪) = 1; we can skip
            return Ok(false);
        }

        // If q = 𝒪 => MillerLoop(P, 𝒪) = 1; we can skip
        if g2_is_inf {
            // Validate p1 field elements and curve membership
            let x1: [u64; 6] = g1[0..6].try_into().unwrap();
            let y1: [u64; 6] = g1[6..12].try_into().unwrap();
            if !lt(&x1, &P) || !lt(&y1, &P) {
                return Err(PAIRING_CHECK_ERR_G1_NOT_IN_FIELD);
            }
            if !is_on_curve_bls12_381(
                g1,
                #[cfg(feature = "hints")]
                hints,
            ) {
                return Err(PAIRING_CHECK_ERR_G1_NOT_ON_CURVE);
            }
            if !is_on_subgroup_bls12_381(
                g1,
                #[cfg(feature = "hints")]
                hints,
            ) {
                return Err(PAIRING_CHECK_ERR_G1_NOT_IN_SUBGROUP);
            }
            return Ok(false);
        }

        // If p = 𝒪 => MillerLoop(𝒪, Q) = 1; we can skip
        if g1_is_inf {
            // Validate p2 field elements and curve membership
            let x2_0: [u64; 6] = g2[0..6].try_into().unwrap();
            let x2_1: [u64; 6] = g2[6..12].try_into().unwrap();
            let y2_0: [u64; 6] = g2[12..18].try_into().unwrap();
            let y2_1: [u64; 6] = g2[18..24].try_into().unwrap();
            if !lt(&x2_0, &P) || !lt(&x2_1, &P) || !lt(&y2_0, &P) || !lt(&y2_1, &P) {
                return Err(PAIRING_CHECK_ERR_G2_NOT_IN_FIELD);
            }
            if !is_on_curve_twist_bls12_381(
                g2,
                #[cfg(feature = "hints")]
                hints,
            ) {
                return Err(PAIRING_CHECK_ERR_G2_NOT_ON_CURVE);
            }
            if !is_on_subgroup_twist_bls12_381(
                g2,
                #[cfg(feature = "hints")]
                hints,
            ) {
                return Err(PAIRING_CHECK_ERR_G2_NOT_IN_SUBGROUP);
            }
            return Ok(false);
        }

        // Both points are non-identity, validate both
        let x1: [u64; 6] = g1[0..6].try_into().unwrap();
        let y1: [u64; 6] = g1[6..12].try_into().unwrap();
        if !lt(&x1, &P) || !lt(&y1, &P) {
            return Err(PAIRING_CHECK_ERR_G1_NOT_IN_FIELD);
        }
        if !is_on_curve_bls12_381(
            g1,
            #[cfg(feature = "hints")]
            hints,
        ) {
            return Err(PAIRING_CHECK_ERR_G1_NOT_ON_CURVE);
        }
        if !is_on_subgroup_bls12_381(
            g1,
            #[cfg(feature = "hints")]
            hints,
        ) {
            return Err(PAIRING_CHECK_ERR_G1_NOT_IN_SUBGROUP);
        }

        let x2_0: [u64; 6] = g2[0..6].try_into().unwrap();
        let x2_1: [u64; 6] = g2[6..12].try_into().unwrap();
        let y2_0: [u64; 6] = g2[12..18].try_into().unwrap();
        let y2_1: [u64; 6] = g2[18..24].try_into().unwrap();
        if !lt(&x2_0, &P) || !lt(&x2_1, &P) || !lt(&y2_0, &P) || !lt(&y2_1, &P) {
            return Err(PAIRING_CHECK_ERR_G2_NOT_IN_FIELD);
        }
        if !is_on_curve_twist_bls12_381(
            g2,
            #[cfg(feature = "hints")]
            hints,
        ) {
            return Err(PAIRING_CHECK_ERR_G2_NOT_ON_CURVE);
        }
        if !is_on_subgroup_twist_bls12_381(
            g2,
            #[cfg(feature = "hints")]
            hints,
        ) {
            return Err(PAIRING_CHECK_ERR_G2_NOT_IN_SUBGROUP);
        }

    Ok(true)
}

/// BLS12-381 pairing check for big-endian byte format.
///
/// # Input format
/// Per pair: 288 bytes = 96 bytes G1 point + 192 bytes G2 point (big-endian)
/// - G1 point: 48 bytes x + 48 bytes y
/// - G2 point: 48 bytes x_i + 48 bytes x_r + 48 bytes y_i + 48 bytes y_r
///
/// # Safety
/// `pairs` must point to an array of `num_pairs * 288` bytes
///
/// # Returns
/// - [PAIRING_CHECK_SUCCESS] = pairing check passed
/// - [PAIRING_CHECK_FAILED] = pairing check failed
/// - [PAIRING_CHECK_ERR_G1_NOT_IN_FIELD] = error (at least one G1 point coordinate not in field)
/// - [PAIRING_CHECK_ERR_G1_NOT_ON_CURVE] = error (at least one G1 point not on curve)
/// - [PAIRING_CHECK_ERR_G1_NOT_IN_SUBGROUP] = error (at least one G1 point not in subgroup)
/// - [PAIRING_CHECK_ERR_G2_NOT_IN_FIELD] = error (at least one G2 point coordinate not in field)
/// - [PAIRING_CHECK_ERR_G2_NOT_ON_CURVE] = error (at least one G2 point not on curve)
/// - [PAIRING_CHECK_ERR_G2_NOT_IN_SUBGROUP] = error (at least one G2 point not in subgroup)
#[allow(dead_code)]
#[inline]
pub(crate) unsafe fn bls12_381_pairing_check_c(
    pairs: *const u8,
    num_pairs: usize,
    #[cfg(feature = "hints")] hints: &mut Vec<u64>,
) -> u8 {
    let pairs = (0..num_pairs).filter_map(|i| unsafe {
        let pair_ptr = pairs.add(i * 288);
        let g1_bytes: &[u8; 96] = &*(pair_ptr as *const [u8; 96]);
        let g2_bytes: &[u8; 192] = &*(pair_ptr.add(96) as *const [u8; 192]);
        let g1 = g1_bytes_be_to_u64_le_bls12_381(g1_bytes);
        let g2 = g2_bytes_be_to_u64_le_bls12_381(g2_bytes);
        match pairing_check_bls12_381(&g1, &g2) {
            Ok(true) => Some(Ok((g1, g2))),
            Ok(false) => None,
            Err(c) => Some(Err(c)),
        }
    });
    match pairing_batch_bls12_381(pairs) {
        Ok(result) if is_one(&result) => PAIRING_CHECK_SUCCESS,
        Ok(_) => PAIRING_CHECK_FAILED,
        Err(c) => c,
    }
}
