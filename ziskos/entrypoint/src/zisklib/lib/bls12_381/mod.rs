//! Operations on the BLS12-381 pairing-friendly elliptic curve.
//!
//! ## Point arithmetic
//! - [`curve`] — Point arithmetic for G1.
//! - [`twist`] — Point arithmetic for G2.
//!
//! ## Field arithmetic
//! - [`fp`] — Base field Fp (384-bit prime field).
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
//!
//! ## KZG and hash-to-curve
//! - [`kzg`] — KZG polynomial commitment proof verification.
//! - [`map_to_curve`] — Hash-to-curve for G1 and G2.

/// Static batch size for the Miller-loop scratch. `bls12_381_pairing_check_c` chunks an
/// arbitrary number of input pairs into groups of this size, calling `miller_loop_batch_bls12_381`
/// on each and multiplying the per-chunk fp12 results into a single accumulator before one
/// final exponentiation.
///
/// Each unit adds 672 B of stack scratch (g1 96 + g2 192 + xp' 96 + yp' 96 + r 192).
pub(crate) const PAIRING_BATCH_BLS12: usize = 16;

mod constants;
mod curve;
mod cyclotomic;
mod final_exp;
mod fp;
mod fp12;
mod fp2;
mod fp6;
mod fr;
mod kzg;
mod map_to_curve;
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
pub use kzg::*;
pub use map_to_curve::*;
pub use pairing::*;
pub use twist::*;
