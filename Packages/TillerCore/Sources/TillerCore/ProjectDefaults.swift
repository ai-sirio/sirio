import Foundation

/// Pure resolution rule for where new projects (clone/create) land by
/// default — no filesystem I/O, same contract as `WorktreeDefaults`.
public enum ProjectDefaults {
    public static func defaultProjectsRoot(home: String = NSHomeDirectory()) -> String {
        (home as NSString).appendingPathComponent("Tiller/projects")
    }
}
