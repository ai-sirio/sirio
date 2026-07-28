import Foundation
import Observation
import TillerCore

@Observable @MainActor
public final class CodeDocument {
    public let fileURL: URL
    public var text: String
    public private(set) var savedText: String
    public private(set) var externalChangeConflict = false
    public private(set) var fileDeleted: Bool
    public var isDirty: Bool { text != savedText }

    @ObservationIgnored private var monitor: FileSystemEventMonitor?
    @ObservationIgnored private var monitorTask: Task<Void, Never>?

    public init(fileURL: URL) throws {
        let url = fileURL.standardizedFileURL
        self.fileURL = url
        // Check and read through the standardized URL — the raw one may carry `..`
        // or symlink segments that resolve differently from what is stored.
        if FileManager.default.fileExists(atPath: url.path) {
            let content = try String(contentsOf: url, encoding: .utf8)
            text = content
            savedText = content
            fileDeleted = false
            startWatching()
        } else {
            text = ""
            savedText = ""
            fileDeleted = true
        }
    }

    public func save() throws {
        try text.write(to: fileURL, atomically: true, encoding: .utf8)
        savedText = text
        fileDeleted = false
        externalChangeConflict = false
        startWatching()
    }

    public func reloadFromDisk() {
        guard let disk = try? String(contentsOf: fileURL, encoding: .utf8) else { return }
        text = disk
        savedText = disk
        fileDeleted = false
        externalChangeConflict = false
    }

    public func keepLocalBuffer() {
        if let disk = try? String(contentsOf: fileURL, encoding: .utf8) { savedText = disk }
        externalChangeConflict = false
    }

    public func stopWatching() {
        monitorTask?.cancel()
        monitorTask = nil
        monitor?.stop()
        monitor = nil
    }

    func handleExternalChange() {
        guard let disk = try? String(contentsOf: fileURL, encoding: .utf8),
              disk != savedText else { return }
        if isDirty { externalChangeConflict = true }
        else { text = disk; savedText = disk }
    }

    func handleFileGone() {
        stopWatching()
        fileDeleted = true
    }

    private func startWatching() {
        stopWatching()
        guard let monitor = FileSystemEventMonitor(roots: [fileURL]) else { return }
        self.monitor = monitor
        let watchedURL = fileURL
        monitorTask = Task { [weak self, monitor] in
            for await urls in monitor.events {
                guard !Task.isCancelled else { return }
                guard urls.contains(where: { $0.standardizedFileURL == watchedURL }) else { continue }
                guard let self else { return }
                if FileManager.default.fileExists(atPath: watchedURL.path) {
                    self.handleExternalChange()
                } else {
                    self.handleFileGone()
                    return
                }
            }
        }
    }
}
