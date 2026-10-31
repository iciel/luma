//! Expose the shared C header to libwebp's build script.
use std::{env, path::PathBuf};

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    println!("cargo:include={}", root.join("include").display());
    println!("cargo:rerun-if-changed=include");
}
