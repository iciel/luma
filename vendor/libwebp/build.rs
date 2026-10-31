//! Build native libwebp with shared math, no threads or SIMD, and no fused multiply-add.

use std::{env, fs, path::PathBuf};

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let webp = root.join("libwebp");
    let clib = PathBuf::from(env::var("DEP_PNGDEER_CLIB_INCLUDE").unwrap());
    let mut build = cc::Build::new();
    build
        .include(&webp)
        // libwebp/src/webp/config.h: no WEBP_HAVE_SSE2/NEON/..., so no SIMD code (its results can
        // differ from the plain C's), and no WEBP_USE_THREAD
        .define("HAVE_CONFIG_H", None)
        .define("NDEBUG", None)
        .warnings(false);
    let header = clib.join("pngdeer_clib.h");
    if build.get_compiler().is_like_msvc() {
        // VS 2022 precise mode leaves contraction off unless /fp:contract is enabled.
        // Strict mode rejects sharpyuv's static initializer `1. / 0.45` (C2099).
        build.flag("/fp:precise").flag(format!("/FI{}", header.display()));
    } else {
        // clang fuses a * b + c where the target has FMA (arm64), and a fused result differs
        build.flag("-ffp-contract=off").flag("-include").flag(header.to_str().unwrap());
    }
    build.file(root.join("glue/pngdeer_webp.c"));
    for dir in ["src/dec", "src/dsp", "src/enc", "src/utils", "sharpyuv"] {
        let mut files: Vec<PathBuf> =
            fs::read_dir(webp.join(dir)).unwrap().map(|entry| entry.unwrap().path()).filter(|path| path.extension().is_some_and(|e| e == "c")).collect();
        files.sort();
        build.files(files);
    }
    build.compile("webp");
    println!("cargo:rerun-if-changed=libwebp");
    println!("cargo:rerun-if-changed=glue");
}
