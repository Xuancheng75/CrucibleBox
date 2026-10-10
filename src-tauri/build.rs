fn main() {
    // tauri-build tracks configuration/resources but not the Windows icon.
    // Recompile the native resource when the icon changes on incremental builds.
    println!("cargo:rerun-if-changed=icons/icon.ico");
    tauri_build::build()
}
