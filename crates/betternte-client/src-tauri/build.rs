fn main() {
    #[cfg(windows)]
    emit_opencv_link_hints();

    let mut attrs = tauri_build::Attributes::new();

    #[cfg(windows)]
    if !tauri_build::is_dev() {
        let mut windows = tauri_build::WindowsAttributes::new();
        windows = windows.app_manifest(include_str!("windows_app_manifest.xml"));
        attrs = attrs.windows_attributes(windows);
    }

    tauri_build::try_build(attrs).expect("failed to run tauri-build");
}

/// Re-emit OpenCV link hints for the **root** `cdylib` link on MSVC.
///
/// `opencv` / `opencv-sys` set `cargo:rustc-link-*` when building `betternte-vision`, but the
/// final `betternte_client` DLL link sometimes does not pick up the OpenCV import library. CI and
/// local builds set `OPENCV_LINK_PATHS` / `OPENCV_LINK_LIBS` (see `.github/workflows/build-tauri-msi.yml`);
/// mirroring them here keeps `link.exe` satisfied.
#[cfg(windows)]
fn emit_opencv_link_hints() {
    println!("cargo:rerun-if-env-changed=OPENCV_LINK_PATHS");
    println!("cargo:rerun-if-env-changed=OPENCV_LINK_LIBS");

    if let Ok(paths) = std::env::var("OPENCV_LINK_PATHS") {
        for p in paths.split(';') {
            let p = p.trim();
            if !p.is_empty() {
                println!("cargo:rustc-link-search=native={p}");
            }
        }
    }

    if let Ok(libs) = std::env::var("OPENCV_LINK_LIBS") {
        for lib in libs.split(',') {
            let lib = lib.trim();
            if lib.is_empty() {
                continue;
            }
            let stem = lib.strip_suffix(".lib").unwrap_or(lib);
            println!("cargo:rustc-link-lib=dylib={stem}");
        }
    }
}
