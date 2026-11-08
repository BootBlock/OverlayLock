//! Embeds the application manifest, the icon and the version resource into `overlay-lock.exe`.

const MANIFEST: &str = "assets/overlay-lock.manifest";
const ICON: &str = "assets/overlay-lock.ico";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={MANIFEST}");
    println!("cargo:rerun-if-changed={ICON}");

    let mut res = winresource::WindowsResource::new();
    res.set_manifest_file(MANIFEST)
        .set_icon(ICON)
        .set("ProductName", "OverlayLock")
        .set("FileDescription", "OverlayLock")
        .set("InternalName", "overlay-lock")
        .set("OriginalFilename", "overlay-lock.exe")
        .set("LegalCopyright", "Copyright (c) 2026 Joe Cox");
    if let Err(error) = res.compile() {
        panic!("failed to compile the Windows resources: {error}");
    }
}
