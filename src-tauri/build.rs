fn main() {
    // Pass the target triple to the build so sidecar binary names can be constructed
    println!(
        "cargo:rustc-env=TARGET={}",
        std::env::var("TARGET").unwrap()
    );
    tauri_build::build();
}
