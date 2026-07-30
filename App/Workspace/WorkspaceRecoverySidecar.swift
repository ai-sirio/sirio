import Foundation
import TillerCore

struct WorkspaceRecoverySidecarEnvelope: Codable, Sendable, Equatable {
    let schemaVersion: Int
    let revision: Int
    let payload: String
    let checksum: String

    var payloadData: Data { Data(payload.utf8) }
}

enum WorkspaceRecoverySidecar {
    static func write(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot,
                      directory: URL? = nil) throws {
        let payload = try snapshot.canonicalPayload()
        let envelope = WorkspaceRecoverySidecarEnvelope(
            schemaVersion: snapshot.schemaVersion, revision: revision,
            payload: String(decoding: payload, as: UTF8.self), checksum: SHA256Hex.digest(payload))
        try write(envelope: envelope, worktreeID: worktreeID, directory: directory)
    }

    static func write(envelope: WorkspaceRecoverySidecarEnvelope, worktreeID: UUID,
                      directory: URL? = nil) throws {
        let directory = try directory ?? defaultDirectory()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let data = try JSONEncoder().encode(envelope)
        try data.write(to: url(worktreeID: worktreeID, directory: directory), options: .atomic)
    }

    static func read(worktreeID: UUID, directory: URL? = nil) throws -> WorkspaceRecoverySidecarEnvelope? {
        let directory = try directory ?? defaultDirectory()
        let sidecarURL = url(worktreeID: worktreeID, directory: directory)
        guard FileManager.default.fileExists(atPath: sidecarURL.path) else { return nil }
        let data = try Data(contentsOf: sidecarURL)
        let envelope = try JSONDecoder().decode(WorkspaceRecoverySidecarEnvelope.self, from: data)
        guard SHA256Hex.digest(envelope.payloadData) == envelope.checksum else {
            throw WorkspaceRecoverySidecarError.invalidChecksum
        }
        return envelope
    }

    static func quarantine(worktreeID: UUID, directory: URL? = nil) throws {
        let directory = try directory ?? defaultDirectory()
        let sidecarURL = url(worktreeID: worktreeID, directory: directory)
        guard FileManager.default.fileExists(atPath: sidecarURL.path) else { return }
        let quarantineDirectory = directory.appendingPathComponent("quarantine", isDirectory: true)
        try FileManager.default.createDirectory(at: quarantineDirectory, withIntermediateDirectories: true)
        let target = quarantineDirectory.appendingPathComponent(
            "\(worktreeID.uuidString)-\(UUID().uuidString).json")
        try FileManager.default.moveItem(at: sidecarURL, to: target)
    }

    static func url(worktreeID: UUID, directory: URL) -> URL {
        directory.appendingPathComponent("\(worktreeID.uuidString).json")
    }

    private static func defaultDirectory() throws -> URL {
        try FileManager.default.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil,
            create: true
        ).appendingPathComponent("Tiller/recovery", isDirectory: true)
    }
}

enum WorkspaceRecoverySidecarError: Error, Equatable, Sendable {
    case invalidChecksum
}
