fn main() {
    // This crate is linked by host apps, so package-wide manifests collide with host manifests.
    println!("cargo:rerun-if-changed=build.rs");
}
