#![cfg_attr(zisk_guest, no_std)]
#![cfg_attr(zisk_guest, feature(core_intrinsics))]
#![cfg_attr(zisk_guest, allow(internal_features))]

// This crate produces libziskos.a for linking by C programs.
// Re-exporting the public interface ensures those symbols are bundled into the
// archive.  The #[panic_handler] is required by staticlib but not rlib targets.

pub use ziskos::zisklib::zkvm_accelerators::*;
pub use ziskos::zisklib::zkvm_io::read_input;
pub use ziskos::zisklib::zkvm_io::write_output;
pub use ziskos::zkvm_deinit;
pub use ziskos::zkvm_init;

#[cfg(all(feature = "panic-handler", zisk_guest))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    core::intrinsics::abort()
}

#[cfg(zisk_guest)]
struct Alloc;

#[cfg(zisk_guest)]
#[global_allocator]
static ALLOC: Alloc = Alloc;

#[cfg(zisk_guest)]
unsafe impl core::alloc::GlobalAlloc for Alloc {
    unsafe fn alloc(&self, _: core::alloc::Layout) -> *mut u8 {
        core::intrinsics::abort()
    }

    unsafe fn dealloc(&self, _: *mut u8, _: core::alloc::Layout) {
        core::intrinsics::abort()
    }
}
