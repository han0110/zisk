#![cfg_attr(all(target_os = "zkvm", target_vendor = "zisk"), no_std)]
#![cfg_attr(all(target_os = "zkvm", target_vendor = "zisk"), feature(core_intrinsics))]
#![cfg_attr(all(target_os = "zkvm", target_vendor = "zisk"), allow(internal_features))]

// This crate produces libziskos.a for linking by C programs.
// Re-exporting the public interface ensures those symbols are bundled into the
// archive.  The #[panic_handler] is required by staticlib but not rlib targets.

pub use ziskos::zisklib::zkvm_accelerators::*;
pub use ziskos::zisklib::zkvm_io::read_input;
pub use ziskos::zisklib::zkvm_io::write_output;
pub use ziskos::zkvm_deinit;
pub use ziskos::zkvm_init;

#[cfg(all(feature = "panic-handler", target_os = "zkvm", target_vendor = "zisk"))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    core::intrinsics::abort()
}

#[cfg(all(target_os = "zkvm", target_vendor = "zisk"))]
mod stub_alloc {
    struct StubAlloc;

    #[global_allocator]
    static ALLOC: StubAlloc = StubAlloc;

    unsafe impl core::alloc::GlobalAlloc for StubAlloc {
        unsafe fn alloc(&self, _: core::alloc::Layout) -> *mut u8 {
            core::intrinsics::abort()
        }
        unsafe fn dealloc(&self, _: *mut u8, _: core::alloc::Layout) {}
    }
}
