//! Windows link workaround for `libghostty-vt-sys` 0.2.1.
//!
//! The `-sys` crate's build script emits `cargo:rustc-link-lib=static=ghostty-vt`
//! in its vendored (default) mode. On MSVC that resolves to `ghostty-vt.lib`,
//! which is the *import* library for `ghostty-vt.dll` — the truly static
//! archive is `ghostty-vt-static.lib` (the crate's own `matches_library` logic
//! knows this, but the emitted link directive doesn't). The result was an exe
//! with a load-time dependency on a DLL sitting in the build-script OUT_DIR:
//! every binary linked against this crate failed to start with a
//! "ghostty-vt.dll not found" hard-error dialog, hanging the app before
//! `main` (#44 "the app runs").
//!
//! This script finds the vendored install's `ghostty-vt-static.lib`, links it
//! explicitly, and excludes the import library from the link. Unix is
//! untouched: `static=ghostty-vt` correctly resolves to `libghostty-vt.a`
//! there. The defect is reported upstream as
//! <https://github.com/Uzaaft/libghostty-rs/issues/78>; drop this file once a
//! `libghostty-vt-sys` release ships with that issue fixed.

use std::path::PathBuf;

fn main() {
    if !cfg!(target_os = "windows") {
        return;
    }
    // OUT_DIR is <target>/<profile>/build/<crate>-<hash>/out, so three levels
    // up is the profile dir that also holds build/.
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let Some(profile_dir) = out_dir.ancestors().nth(3) else {
        return;
    };
    let static_lib = std::fs::read_dir(profile_dir.join("build"))
        .ok()
        .and_then(|entries| {
            entries.filter_map(Result::ok).find_map(|entry| {
                let candidate = entry
                    .path()
                    .join("out/ghostty-install/lib/ghostty-vt-static.lib");
                candidate.is_file().then_some(candidate)
            })
        });
    let Some(static_lib) = static_lib else {
        println!(
            "cargo:warning=sirio_terminal: ghostty-vt-static.lib not found under {}; \
             falling back to libghostty-vt-sys's own (import) link",
            profile_dir.join("build").display()
        );
        return;
    };
    println!("cargo:rerun-if-changed={}", static_lib.display());
    println!(
        "cargo:rustc-link-search=native={}",
        static_lib
            .parent()
            .expect("static lib has a parent")
            .display()
    );
    // The real static archive, put on the link line beside the import library
    // the `-sys` crate already named.
    //
    // #179: this crate cannot do more than that. It used to also emit
    // `cargo:rustc-link-arg=/NODEFAULTLIB:ghostty-vt.lib`, which was inert
    // twice over — `rustc-link-arg` applies only to the crate being built and
    // a library crate has no link step, and `/NODEFAULTLIB` suppresses only
    // libraries named by `/DEFAULTLIB` directives in object files, never one
    // rustc passes explicitly. With no working exclusion, which archive won
    // was left to the linker's tie-break; debug happened to pick this one and
    // release did not, shipping a binary that could not start.
    //
    // The exclusion now lives in `sirio/build.rs`, the crate that actually
    // links, as `/WHOLEARCHIVE:ghostty-vt-static.lib`. See the comment there.
    println!("cargo:rustc-link-lib=static=ghostty-vt-static");
}
