import Foundation

/// Doppio gate per l'auto-naming: non ripartire prima di `minInterval`
/// secondi *e* prima che il transcript sia cresciuto di almeno `minGrowth`
/// caratteri dall'ultimo pass. Stato puro, tenuto in memoria per pane da
/// AppModel — un riavvio dell'app al più causa un pass extra, mai un bug.
public struct AutoNamingThrottle: Sendable, Equatable {
    public static let minInterval: TimeInterval = 30
    public static let minGrowth = 200

    public var lastRunAt: Date?
    public var lastTranscriptLength: Int

    public init(lastRunAt: Date? = nil, lastTranscriptLength: Int = 0) {
        self.lastRunAt = lastRunAt
        self.lastTranscriptLength = lastTranscriptLength
    }

    public func shouldRun(transcriptLength: Int, now: Date) -> Bool {
        guard let lastRunAt else { return true }
        guard now.timeIntervalSince(lastRunAt) >= Self.minInterval else { return false }
        return transcriptLength - lastTranscriptLength >= Self.minGrowth
    }

    public func recording(transcriptLength: Int, now: Date) -> AutoNamingThrottle {
        AutoNamingThrottle(lastRunAt: now, lastTranscriptLength: transcriptLength)
    }
}
