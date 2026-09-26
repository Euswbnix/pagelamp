//! `uniffi-bindgen-swift`: generates the Swift sources, C header and module map from the built
//! `libpagelamp_ffi.a` (run by apps/macos/scripts/build-ffi.sh; needs `cargo` on PATH).

fn main() {
    uniffi::uniffi_bindgen_swift()
}
