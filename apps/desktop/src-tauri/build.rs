//! Tauri's code generation and Windows resource (icon, version info), plus the Windows app
//! manifest for every executable of this crate.
//!
//! tauri-build puts the Common Controls v6 manifest into its resource, which is linked into the
//! app binary only. Test executables then lack it and fail to load with
//! STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139; TaskDialogIndirect, pulled in by the dialog plugin,
//! exists only in Common Controls v6). On Windows with MSVC the manifest is therefore embedded by
//! the linker into every executable, the app and its unit and integration tests alike, and left
//! out of tauri-build's resource so the app doesn't get two.

use std::env;
use std::path::PathBuf;

fn main() {
    let windows_msvc = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if !windows_msvc {
        tauri_build::build();
        return;
    }

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .join("windows")
        .join("common-controls.manifest");
    println!("cargo:rerun-if-changed={}", manifest.display());
    // `rustc-link-arg` reaches every linked target of this package: bins, the lib's unit-test
    // executable, integration tests (the `-tests`/`-bins` variants miss the lib's unit tests).
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());

    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    if let Err(error) =
        tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
    {
        println!("{error:#}");
        std::process::exit(1);
    }
}
