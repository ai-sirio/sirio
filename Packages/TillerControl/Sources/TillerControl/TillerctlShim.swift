import Foundation

/// Stable on-disk location for tillerctl, decoupled from any specific build.
///
/// Agent hook configs written into worktrees embed an absolute tillerctl
/// path at prepare-time; pointing them at the app bundle breaks the moment
/// that bundle moves (DerivedData clean, app update, dev↔installed switch).
/// Instead hooks reference this shim — a symlink the app re-points to its
/// own bundled binary on every launch.
public enum TillerctlShim {
    /// ~/Library/Application Support/Tiller/bin/tillerctl
    public static func defaultShimPath() -> String {
        let urls = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)
        let appSupport = urls.first ?? URL(fileURLWithPath: NSHomeDirectory())
            .appendingPathComponent("Library/Application Support")
        return appSupport.appendingPathComponent("Tiller/bin/tillerctl").path
    }

    /// Creates or re-points the symlink at `shimPath` to `target`. Idempotent.
    public static func install(target: String, shimPath: String) throws {
        let fm = FileManager.default
        try fm.createDirectory(
            atPath: (shimPath as NSString).deletingLastPathComponent,
            withIntermediateDirectories: true
        )
        // lstat, not stat: a dangling symlink must still be removed.
        if (try? fm.attributesOfItem(atPath: shimPath)) != nil {
            try fm.removeItem(atPath: shimPath)
        }
        try fm.createSymbolicLink(atPath: shimPath, withDestinationPath: target)
    }
}
