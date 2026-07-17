import os
import Foundation
import TillerCore

/// os_signpost instrumentation for the terminal pipeline, gated by
/// `UserDefaults.standard.bool(forKey: "debug.signpostMetrics")`.
///
/// When the gate is off, all calls are compile-time no-ops — zero runtime
/// overhead. When on, signposts are visible in Instruments (os_signpost
/// template) and via `oslog` with `OS_LOG_TYPE_DEBUG`.
///
/// No sensitive content (terminal output, paths, commands, identities) is
/// ever included in signpost payloads — only fixed labels and numeric counts.
public enum SignpostMetrics {
    private static let enabled = { UserDefaults.standard.bool(forKey: AppSettings.signpostMetricsKey) }()
    private static let log = OSLog(subsystem: "dev.tiller", category: .pointsOfInterest)
    private static let signposter = OSSignposter(logHandle: log)

    public static func beginInterval(_ name: StaticString, id: OSSignpostID = .exclusive) -> OSSignpostIntervalState? {
        guard enabled else { return nil }
        return signposter.beginInterval(name, id: id)
    }

    public static func endInterval(_ name: StaticString, _ state: OSSignpostIntervalState?, message: String? = nil) {
        guard enabled, let state else { return }
        if let message {
            signposter.endInterval(name, state, "\(message)")
        } else {
            signposter.endInterval(name, state)
        }
    }

    public static func makeSignpostID() -> OSSignpostID {
        signposter.makeSignpostID()
    }
}
