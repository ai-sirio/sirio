//! Windows-only: embeds `assets/app-icon/app-icon.ico` as the process icon
//! resource (ID 1), plus the tray attention icon as resource ID 2. The gpui
//! Windows backend loads ID 1 once via `LoadImageW` and registers it as the
//! window-class icon (`WNDCLASS.hIcon`).
//! Every other platform is covered elsewhere: X11 takes the icon straight
//! from `WindowOptions.icon` (`_NET_WM_ICON`), Wayland has no per-window
//! icon API, and macOS gets it from the .app bundle's AppIcon.

fn main() {
    #[cfg(target_os = "windows")]
    {
        // #179: keep `ghostty-vt.dll` out of this binary's imports.
        //
        // `libghostty-vt-sys` emits `cargo:rustc-link-lib=static=ghostty-vt`,
        // which on MSVC names `ghostty-vt.lib` — the *import* library for the
        // DLL. The real archive is `ghostty-vt-static.lib`, and
        // `tiller_terminal/build.rs` puts it on the link line. Both therefore
        // reach the linker, and which one satisfies the symbols is left to its
        // tie-break: debug happened to pick the archive, release picked the
        // import and shipped a binary that died at load with
        // STATUS_DLL_NOT_FOUND, no window and no output on either stream.
        //
        // `/WHOLEARCHIVE` pulls every object out of the static archive whether
        // or not it is referenced, so by the time the import library is
        // consulted there is nothing left for it to resolve and no import is
        // emitted. Two things that look like they would do this and do not:
        // `/NODEFAULTLIB:ghostty-vt.lib` only suppresses libraries named by
        // `/DEFAULTLIB` directives inside object files, not ones rustc passes
        // explicitly; and emitting either flag from `tiller_terminal` is inert
        // because `cargo:rustc-link-arg` applies to the crate being built and a
        // library crate has no link step. It has to be emitted here, by the
        // crate that actually links.
        //
        // Remove once a `libghostty-vt-sys` release emits
        // `static=ghostty-vt-static` on Windows — upstream issue
        // <https://github.com/Uzaaft/libghostty-rs/issues/78>.
        println!("cargo:rustc-link-arg=/WHOLEARCHIVE:ghostty-vt-static.lib");

        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        let icon = std::path::Path::new(&manifest_dir).join("../../assets/app-icon/app-icon.ico");
        let attention_icon = std::path::Path::new(&manifest_dir)
            .join("../../assets/app-icon/app-icon-attention.ico");
        let rc = std::path::Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR"))
            .join("tiller.rc");
        // Absolute path, written into the .rc: `rc.exe` resolves the ICON
        // argument against its own working directory, not the .rc's. And
        // `rc.exe` treats `\` as a C-style escape inside the filename, so
        // the backslashes must be doubled (`\P` would otherwise read as a
        // TAB and the icon would "not be found").
        let icon_literal = icon.display().to_string().replace('\\', "\\\\");
        let attention_icon_literal = attention_icon.display().to_string().replace('\\', "\\\\");
        std::fs::write(
            &rc,
            format!(
                "1 ICON \"{icon_literal}\"\n2 ICON \"{attention_icon_literal}\"\n"
            ),
        )
        .expect("write tiller.rc");
        // `compile` returns a must-use `CompilationResult`; a missing
        // `rc.exe` (`NotAttempted`) or a bad resource (`Failed`) must fail
        // the build loudly, not leave Windows on the default icon.
        embed_resource::compile(&rc, embed_resource::NONE)
            .manifest_required()
            .expect("compile the app-icon resource into the binary");
    }
}