import Foundation

/// Registry `binary` distribution platform keys for the current host.
public enum HostPlatform: String, Sendable {
    case darwinArm64 = "darwin-aarch64"
    case darwinX86 = "darwin-x86_64"

    public static var current: HostPlatform {
        #if arch(arm64)
        .darwinArm64
        #else
        .darwinX86
        #endif
    }
}

/// Concrete way to install one registry agent on this machine.
public enum InstallMethod: Sendable, Equatable {
    case npx(package: String, args: [String], env: [String: String])
    case binary(archive: URL, cmd: String, args: [String], env: [String: String])
}

extension RegistryAgent {
    /// npx wins when both exist (no archive download needed). Nil when the
    /// agent is uvx-only or has no archive for this platform.
    public func installMethod(platform: HostPlatform) -> InstallMethod? {
        if let npx = distribution.npx {
            return .npx(package: npx.package, args: npx.args ?? [],
                        env: npx.env ?? [:])
        }
        if let entry = distribution.binary?[platform.rawValue] {
            return .binary(archive: entry.archive, cmd: entry.cmd,
                           args: entry.args ?? [], env: entry.env ?? [:])
        }
        return nil
    }
}

/// `installed.json` written after a successful install; single source of
/// truth for "installed" state and for launching.
public struct InstalledAgentManifest: Sendable, Equatable, Codable {
    public var id: String
    public var version: String
    public var executable: String
    public var arguments: [String]
    public var environment: [String: String]

    public init(id: String, version: String, executable: String,
                arguments: [String], environment: [String: String]) {
        self.id = id
        self.version = version
        self.executable = executable
        self.arguments = arguments
        self.environment = environment
    }
}

/// Filesystem layout of Tiller-managed agent installs:
/// `<root>/<id>/installed.json` + the install payload next to it.
public struct AgentInstallStore: Sendable {
    public let rootDirectory: URL

    public init(rootDirectory: URL) {
        self.rootDirectory = rootDirectory
    }

    public func agentDirectory(id: String) -> URL {
        rootDirectory.appendingPathComponent(id, isDirectory: true)
    }

    public func manifestURL(id: String) -> URL {
        agentDirectory(id: id).appendingPathComponent("installed.json")
    }

    public func manifest(id: String) -> InstalledAgentManifest? {
        guard let data = try? Data(contentsOf: manifestURL(id: id)) else { return nil }
        return try? JSONDecoder().decode(InstalledAgentManifest.self, from: data)
    }

    public func installedManifests() -> [InstalledAgentManifest] {
        let contents = (try? FileManager.default.contentsOfDirectory(
            at: rootDirectory, includingPropertiesForKeys: nil)) ?? []
        return contents
            .compactMap { manifest(id: $0.lastPathComponent) }
            .sorted { $0.id < $1.id }
    }

    public func write(_ manifest: InstalledAgentManifest) throws {
        let dir = agentDirectory(id: manifest.id)
        try FileManager.default.createDirectory(
            at: dir, withIntermediateDirectories: true)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(manifest).write(to: manifestURL(id: manifest.id),
                                           options: .atomic)
    }
}

/// Install state shown in Settings, derived — never stored.
public enum AgentInstallStatus: Sendable, Equatable {
    case notInstalled
    case installed(version: String)
    case updateAvailable(installed: String, latest: String)
    case unsupported

    public static func resolve(manifest: InstalledAgentManifest?,
                               latestVersion: String?,
                               isSupported: Bool) -> AgentInstallStatus {
        guard let manifest else {
            return isSupported ? .notInstalled : .unsupported
        }
        if let latestVersion, latestVersion != manifest.version {
            return .updateAvailable(installed: manifest.version, latest: latestVersion)
        }
        return .installed(version: manifest.version)
    }
}
