//! Windows-only: embeds `assets/app-icon/app-icon.ico` as the process icon
//! resource (ID 1), which the gpui Windows backend loads once via
//! `LoadImageW` and registers as the window-class icon (`WNDCLASS.hIcon`).
//! Every other platform is covered elsewhere: X11 takes the icon straight
//! from `WindowOptions.icon` (`_NET_WM_ICON`), Wayland has no per-window
//! icon API, and macOS gets it from the .app bundle's AppIcon.

fn main() {
    #[cfg(target_os = "windows")]
    {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        let icon = std::path::Path::new(&manifest_dir).join("../../assets/app-icon/app-icon.ico");
        let rc = std::path::Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR"))
            .join("tiller.rc");
        // Absolute path, written into the .rc: `rc.exe` resolves the ICON
        // argument against its own working directory, not the .rc's. And
        // `rc.exe` treats `\` as a C-style escape inside the filename, so
        // the backslashes must be doubled (`\P` would otherwise read as a
        // TAB and the icon would "not be found").
        let icon_literal = icon.display().to_string().replace('\\', "\\\\");
        std::fs::write(&rc, format!("1 ICON \"{icon_literal}\"\n"))
            .expect("write tiller.rc");
        // `compile` returns a must-use `CompilationResult`; a missing
        // `rc.exe` (`NotAttempted`) or a bad resource (`Failed`) must fail
        // the build loudly, not leave Windows on the default icon.
        embed_resource::compile(&rc, embed_resource::NONE)
            .manifest_required()
            .expect("compile the app-icon resource into the binary");
    }
}