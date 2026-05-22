//! Operations on the BN254 (alt_bn128) pairing-friendly elliptic curve.
//!
//! ## Point arithmetic
//! - [`curve`] — Point arithmetic for G1.
//! - [`twist`] — Point arithmetic for G2.
//!
//! ## Field arithmetic
//! - [`fp`] — Base field Fp (256-bit prime field).
//! - [`fr`] — Scalar field Fr.
//! - [`fp2`] — Degree-2 extension Fp2.
//! - [`fp6`] — Degree-6 extension Fp6.
//! - [`fp12`] — Degree-12 extension Fp12.
//!
//! ## Pairing
//! - [`miller_loop`] — Miller loop computation.
//! - [`final_exp`] — Final exponentiation.
//! - [`cyclotomic`] — Cyclotomic subgroup arithmetic.
//! - [`pairing`] — Optimal Ate pairing and batch pairing check.

/// Static batch size for the Miller-loop scratch. `bn254_pairing_check_c` chunks an arbitrary
/// number of input pairs into groups of this size, calling `miller_loop_batch_bn254` on each
/// and multiplying the per-chunk fp12 results into a single accumulator before one final exp.
///
/// Each unit adds 384 B of stack scratch (g1 64 + g2 128 + xp' 32 + yp' 32 + r 128).
pub(crate) const PAIRING_BATCH_BN254: usize = 8;

mod constants;
mod curve;
mod cyclotomic;
mod final_exp;
mod fp;
mod fp12;
mod fp2;
mod fp6;
mod fr;
mod miller_loop;
mod pairing;
mod twist;

pub use curve::*;
pub use cyclotomic::*;
pub use final_exp::*;
pub use fp::*;
pub use fp12::*;
pub use fp2::*;
pub use fp6::*;
pub use fr::*;
pub use pairing::*;
pub use twist::*;
