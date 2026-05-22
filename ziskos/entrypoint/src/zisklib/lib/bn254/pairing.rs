//! Pairing over BN254

#[cfg(all(target_os = "zkvm", target_vendor = "zisk"))]
use crate::alloc_extern::vec::Vec;

use crate::zisklib::lib::utils::{eq, is_one, lt};

use super::{
    constants::{G1_IDENTITY, G2_IDENTITY, P},
    curve::{g1_bytes_be_to_u64_le_bn254, is_on_curve_bn254},
    final_exp::final_exp_bn254,
    fp12::mul_fp12_bn254,
    miller_loop::{miller_loop_batch_bn254, miller_loop_bn254},
    twist::{g2_bytes_be_to_u64_le_bn254, is_on_curve_twist_bn254, is_on_subgroup_twist_bn254},
    PAIRING_BATCH_BN254,
};

/// Pairing check result codes
#[allow(dead_code)]
pub(crate) const PAIRING_CHECK_SUCCESS: u8 = 0;
#[allow(dead_code)]
pub(crate) const PAIRING_CHECK_FAILED: u8 = 1;
const PAIRING_CHECK_ERR_G1_NOT_IN_FIELD: u8 = 2;
const PAIRING_CHECK_ERR_G1_NOT_ON_CURVE: u8 = 3;
const PAIRING_CHECK_ERR_G2_NOT_IN_FIELD: u8 = 4;
const PAIRING_CHECK_ERR_G2_NOT_ON_CURVE: u8 = 5;
const PAIRING_CHECK_ERR_G2_NOT_IN_SUBGROUP: u8 = 6;

/// Optimal Ate Pairing e: G1 x G2 -> GT over the BN254 curve
/// where G1 = E(Fp)[r] = E(Fp), G2 = E'(Fp2)[r] and GT = μ_r (the r-th roots of unity over Fp12*
/// the involved curves are E/Fp: y² = x³ + 3 and E'/Fp2: y² = x³ + 3/(9+u)
///  pairingBN254:
///          input: P ∈ G1 and Q ∈ G2
///          output: e(P,Q) ∈ GT
///
pub fn pairing_bn254(
    p: &[u64; 8],
    q: &[u64; 16],
    #[cfg(feature = "hints")] hints: &mut Vec<u64>,
) -> [u64; 48] {
    // Is p = 𝒪?
    if *p == G1_IDENTITY || *q == G2_IDENTITY {
        // e(P, 𝒪) = e(𝒪, Q) = 1;
        let mut one = [0; 48];
        one[0] = 1;
        return one;
    }

    // Miller loop
    let miller_loop = miller_loop_bn254(
        p,
        q,
        #[cfg(feature = "hints")]
        hints,
    );

    // Final exponentiation
    final_exp_bn254(
        &miller_loop,
        #[cfg(feature = "hints")]
        hints,
    )
}

/// Computes the optimal Ate pairing for a batch of G1 and G2 points over the BN254 curve
/// and multiplies the results together, i.e.:
///     e(P₁, Q₁) · e(P₂, Q₂) · ... · e(Pₙ, Qₙ) ∈ GT
pub fn pairing_batch_bn254(
    pairs: impl Iterator<Item = Result<([u64; 8], [u64; 16]), u8>>,
    #[cfg(feature = "hints")] hints: &mut Vec<u64>,
) -> Result<[u64; 48], u8> {
    let mut g1_arr = [[0u64; 8]; PAIRING_BATCH_BN254];
    let mut g2_arr = [[0u64; 16]; PAIRING_BATCH_BN254];
    let mut batch_len: usize = 0;

    let mut f: Option<[u64; 48]> = None;

    for item in pairs {
        let (g1, g2) = item?;
        g1_arr[batch_len] = g1;
        g2_arr[batch_len] = g2;
        batch_len += 1;

        // Chunk full: run Miller loop on this chunk, multiply into accumulator, reset.
        if batch_len == PAIRING_BATCH_BN254 {
            let batch_f = miller_loop_batch_bn254(
                &g1_arr,
                &g2_arr,
                #[cfg(feature = "hints")]
                hints,
            );
            f = Some(match f {
                Some(f) => mul_fp12_bn254(
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

    // Final partial chunk
    if batch_len > 0 {
        let batch_f = miller_loop_batch_bn254(
            &g1_arr[..batch_len],
            &g2_arr[..batch_len],
            #[cfg(feature = "hints")]
            hints,
        );
        f = Some(match f {
            Some(f) => mul_fp12_bn254(
                &f,
                &batch_f,
                #[cfg(feature = "hints")]
                hints,
            ),
            None => batch_f,
        });
    }

    let Some(f) = f else {
        // If all pairing computations were skipped, return 1
        let mut one = [0; 48];
        one[0] = 1;
        return Ok(one);
    };

    Ok(final_exp_bn254(
        &f,
        #[cfg(feature = "hints")]
        hints,
    ))
}

/// BN254 single-pair validation for the pairing check.
///
/// Validates that the points have canonical field elements, are on curve, and the G2 point is in subgroup.
///
/// # Arguments
/// * `g1` - G1 point as [u64; 8]
/// * `g2` - G2 point as [u64; 16]
///
/// # Returns
/// * `Ok(true)` - Pair is well-formed and should be included in the pairing product
/// * `Ok(false)` - Either point is the identity; the pair contributes 1 and can be skipped
/// * `Err(PAIRING_CHECK_ERR_G1_NOT_IN_FIELD)` - G1 field element not canonical (>= P)
/// * `Err(PAIRING_CHECK_ERR_G1_NOT_ON_CURVE)` - G1 point not on curve
/// * `Err(PAIRING_CHECK_ERR_G2_NOT_IN_FIELD)` - G2 field element not canonical (>= P)
/// * `Err(PAIRING_CHECK_ERR_G2_NOT_ON_CURVE)` - G2 point not on twist curve
/// * `Err(PAIRING_CHECK_ERR_G2_NOT_IN_SUBGROUP)` - G2 point not in subgroup
pub fn pairing_check_bn254(
    g1: &[u64; 8],
    g2: &[u64; 16],
    #[cfg(feature = "hints")] hints: &mut Vec<u64>,
) -> Result<bool, u8> {
        let g1_is_inf = eq(g1, &G1_IDENTITY);
        let g2_is_inf = eq(g2, &G2_IDENTITY);

        // If p = 𝒪 or q = 𝒪 => MillerLoop(P, 𝒪) = MillerLoop(𝒪, Q) = 1; we can skip
        if g2_is_inf {
            if !g1_is_inf
                && !is_on_curve_bn254(
                    g1,
                    #[cfg(feature = "hints")]
                    hints,
                )
            {
                return Err(PAIRING_CHECK_ERR_G1_NOT_ON_CURVE);
            }
            return Ok(false);
        }

        if g1_is_inf {
            if !is_on_curve_twist_bn254(
                g2,
                #[cfg(feature = "hints")]
                hints,
            ) {
                return Err(PAIRING_CHECK_ERR_G2_NOT_ON_CURVE);
            }
            if !is_on_subgroup_twist_bn254(
                g2,
                #[cfg(feature = "hints")]
                hints,
            ) {
                return Err(PAIRING_CHECK_ERR_G2_NOT_IN_SUBGROUP);
            }
            return Ok(false);
        }

        // Validate G1 point field elements
        let x1: [u64; 4] = g1[0..4].try_into().unwrap();
        let y1: [u64; 4] = g1[4..8].try_into().unwrap();
        if !lt(&x1, &P) || !lt(&y1, &P) {
            return Err(PAIRING_CHECK_ERR_G1_NOT_IN_FIELD);
        }

        // Verify G1 point is on curve
        if !is_on_curve_bn254(
            g1,
            #[cfg(feature = "hints")]
            hints,
        ) {
            return Err(PAIRING_CHECK_ERR_G1_NOT_ON_CURVE);
        }

        // Validate G2 point field elements
        let x2_r: [u64; 4] = g2[0..4].try_into().unwrap();
        let x2_i: [u64; 4] = g2[4..8].try_into().unwrap();
        let y2_r: [u64; 4] = g2[8..12].try_into().unwrap();
        let y2_i: [u64; 4] = g2[12..16].try_into().unwrap();
        if !lt(&x2_r, &P) || !lt(&x2_i, &P) || !lt(&y2_r, &P) || !lt(&y2_i, &P) {
            return Err(PAIRING_CHECK_ERR_G2_NOT_IN_FIELD);
        }

        // Verify G2 point is on twist curve
        if !is_on_curve_twist_bn254(
            g2,
            #[cfg(feature = "hints")]
            hints,
        ) {
            return Err(PAIRING_CHECK_ERR_G2_NOT_ON_CURVE);
        }

        // Verify G2 point is in subgroup
        if !is_on_subgroup_twist_bn254(
            g2,
            #[cfg(feature = "hints")]
            hints,
        ) {
            return Err(PAIRING_CHECK_ERR_G2_NOT_IN_SUBGROUP);
        }

    Ok(true)
}

// ==================== C FFI Functions ====================

/// Batch optimal Ate pairing over BN254: computes e(P₁,Q₁)·e(P₂,Q₂)·…·e(Pₙ,Qₙ) ∈ GT.
///
/// # Safety
/// - `g1_ptr` must point to `num_pairs * 8` contiguous `u64` values (G1 points, little-endian limbs)
/// - `g2_ptr` must point to `num_pairs * 16` contiguous `u64` values (G2 points, little-endian limbs)
/// - `result_ptr` must point to a writable `[u64; 48]` array
#[cfg_attr(not(feature = "hints"), no_mangle)]
#[cfg_attr(feature = "hints", export_name = "hints_pairing_batch_bn254_c")]
pub unsafe extern "C" fn pairing_batch_bn254_c(
    g1_ptr: *const u64,
    g2_ptr: *const u64,
    num_pairs: usize,
    result_ptr: *mut u64,
    #[cfg(feature = "hints")] hints: &mut Vec<u64>,
) {
    let pairs = (0..num_pairs).map(|i| unsafe {
        let g1 = *(g1_ptr.add(i * 8) as *const [u64; 8]);
        let g2 = *(g2_ptr.add(i * 16) as *const [u64; 16]);
        Ok::<_, u8>((g1, g2))
    });
    let result = &mut *(result_ptr as *mut [u64; 48]);
    *result = unsafe {
        pairing_batch_bn254(
            pairs,
            #[cfg(feature = "hints")]
            hints,
        )
        .unwrap_unchecked()
    };
}

/// BN254 pairing check with big-endian byte format
///
/// # Safety
/// - `pairs` must point to an array of `num_pairs * 192` bytes
///   Each pair is: 64 bytes G1 point + 128 bytes G2 point
///
/// # Returns
/// - 0 = pairing check passed
/// - 1 = pairing check failed
/// - 2 = G1 field element invalid
/// - 3 = G1 point not on curve
/// - 4 = G2 field element invalid
/// - 5 = G2 point not on curve
/// - 6 = G2 point not in subgroup
#[allow(dead_code)]
#[inline]
pub(crate) unsafe fn bn254_pairing_check_c(
    pairs: *const u8,
    num_pairs: usize,
    #[cfg(feature = "hints")] hints: &mut Vec<u64>,
) -> u8 {
    let pairs = (0..num_pairs).filter_map(|i| unsafe {
        let pair_ptr = pairs.add(i * 192);
        let g1_bytes: &[u8; 64] = &*(pair_ptr as *const [u8; 64]);
        let g2_bytes: &[u8; 128] = &*(pair_ptr.add(64) as *const [u8; 128]);
        let g1 = g1_bytes_be_to_u64_le_bn254(g1_bytes);
        let g2 = g2_bytes_be_to_u64_le_bn254(g2_bytes);
        match pairing_check_bn254(&g1, &g2) {
            Ok(true) => Some(Ok((g1, g2))),
            Ok(false) => None,
            Err(c) => Some(Err(c)),
        }
    });
    match pairing_batch_bn254(pairs) {
        Ok(result) if is_one(&result) => PAIRING_CHECK_SUCCESS,
        Ok(_) => PAIRING_CHECK_FAILED,
        Err(c) => c,
    }
}
