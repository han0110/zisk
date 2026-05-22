#[cfg(all(target_os = "zkvm", target_vendor = "zisk"))]
use crate::alloc_extern::vec;
#[cfg(all(target_os = "zkvm", target_vendor = "zisk"))]
use crate::alloc_extern::vec::Vec;

use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(all(target_os = "zkvm", target_vendor = "zisk"))] {
        use core::arch::asm;
        use crate::{ziskos_fcall, ziskos_fcall_get, ziskos_fcall_param};
        use super::FCALL_BIN_DECOMP_ID;
        #[cfg(feature = "inputcpy")]
        use crate::ziskos_inputcpy;
    } else {
        use crate::zisklib::fcalls_impl::bin_decomp::bin_decomp;
    }
}

/// Given an unsigned big integer `x`, it computes the binary decomposition of `x`,
/// writing the individual bits as `u64` values (each 0 or 1) into the caller-provided
/// `bits` slice, from most significant to least significant.
///
/// Returns `len_bits`, the number of bits written; `bits[i]` is the `i`-th bit.
///
/// ### Safety
///
/// The caller must ensure that the input pointer is valid and aligned to an 8-byte boundary.
///
/// Note that this is a *free-input call*, meaning the ZisK VM does not automatically verify the correctness
/// of the result. It is the caller's responsibility to ensure it.
#[allow(unused_variables)]
pub fn fcall_bin_decomp(
    a: &[u64],
    bits: &mut [u64],
    #[cfg(feature = "hints")] hints: &mut Vec<u64>,
) -> usize {
    #[cfg(not(all(target_os = "zkvm", target_vendor = "zisk")))]
    {
        let len_a = a.len();
        let _bits = bin_decomp(a, len_a);
        let len_bits = _bits.len();
        let bits_u64: Vec<u64> = _bits.into_iter().map(|b| b as u64).collect();
        #[cfg(feature = "hints")]
        {
            hints.push(len_bits as u64 + 1);
            hints.push(len_bits as u64);
            hints.extend_from_slice(&bits_u64);
        }

        bits[..bits_u64.len()].copy_from_slice(&bits_u64);

        len_bits
    }
    #[cfg(all(target_os = "zkvm", target_vendor = "zisk"))]
    {
        let len_a = a.len() as usize;
        ziskos_fcall_param!(len_a, 1);
        for i in 0..len_a {
            ziskos_fcall_param!(a[i], 1);
        }

        ziskos_fcall!(FCALL_BIN_DECOMP_ID);

        let len_bits = ziskos_fcall_get() as usize;
        #[cfg(not(feature = "inputcpy"))]
        {
            for i in 0..len_bits {
                bits[i] = ziskos_fcall_get();
            }
            len_bits
        }
        #[cfg(feature = "inputcpy")]
        {
            ziskos_inputcpy!(bits, len_bits * 8);
            len_bits
        }
    }
}
