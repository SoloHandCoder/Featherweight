fn main() {
    // No manifest resource from tauri-build: the admin + Common Controls v6
    // manifest is embedded by the linker into the app binary only, so test
    // executables (lib unit tests) are not marked requireAdministrator.
    // The [[bin]] target has `test = false` in Cargo.toml for the same reason.
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri-build");

    // The tweak catalog is embedded with include_dir!, which cargo doesn't
    // track; rebuild when any file under tweaks/ is added, removed or edited.
    println!("cargo:rerun-if-changed=../tweaks");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("app.manifest");
        println!("cargo:rerun-if-changed=app.manifest");
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:NO");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", manifest.display());
    }
}
