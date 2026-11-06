fn main() {
    // Turns on a build flag for each camera DLL that exists so the app can bundle it.
    for (file, cfg) in [("resources/opencamera_vcam.dll", "vcam_dll"), ("resources/opencamera_vcam32.dll", "vcam_dll32")] {
        println!("cargo::rustc-check-cfg=cfg({cfg})");
        println!("cargo:rerun-if-changed={file}");
        if std::path::Path::new(file).exists() {
            println!("cargo:rustc-cfg={cfg}");
        }
    }
    // Hides the harmless warning about Microsoft's missing C runtime debug symbols.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/IGNORE:4099");
    }
    tauri_build::build()
}
