import Foundation

/// POSIX single-quote escaping: safe for paths with spaces or quotes.
/// Shared with `AgentLaunchSpec.resolved` for the login-shell exec line.
func posixQuoted(_ path: String) -> String {
    "'" + path.replacingOccurrences(of: "'", with: "'\\''") + "'"
}


public struct ShellResult: Sendable, Equatable {
    public var exitCode: Int32
    public var output: String
    public init(exitCode: Int32, output: String) {
        self.exitCode = exitCode
        self.output = output
    }
}

/// Runs a command line through `zsh -lc` so the user's PATH (npm via
/// nvm/homebrew) resolves as in their terminal. Injectable for tests.
public protocol ShellRunning: Sendable {
    func run(_ command: String, cwd: URL) async throws -> ShellResult
}

public struct ZshRunner: ShellRunning {
    public init() {}

    public func run(_ command: String, cwd: URL) async throws -> ShellResult {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/zsh")
        process.arguments = ["-lc", command]
        process.currentDirectoryURL = cwd
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe
        try process.run()
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return ShellResult(exitCode: process.terminationStatus,
                           output: String(decoding: data, as: UTF8.self))
    }
}

public enum AgentInstallError: Error, Equatable {
    case unsupported
    case commandFailed(output: String)
    case noExecutableFound
}

/// Installs registry agents into the Tiller-local store. Staging-and-swap:
/// everything happens in `<root>/<id>.staging`, the final directory is
/// replaced only on success, so a failed install/update never breaks a
/// working one.
public actor AgentInstaller {
    private let store: AgentInstallStore
    private let shell: ShellRunning
    private let download: @Sendable (URL, URL) async throws -> Void

    public init(store: AgentInstallStore, shell: ShellRunning = ZshRunner(),
                download: @escaping @Sendable (URL, URL) async throws -> Void) {
        self.store = store
        self.shell = shell
        self.download = download
    }

    /// Production downloader convenience.
    public init(store: AgentInstallStore, shell: ShellRunning = ZshRunner(),
                session: URLSession = .shared) {
        self.init(store: store, shell: shell, download: { remote, destination in
            let (temp, _) = try await session.download(from: remote)
            try? FileManager.default.removeItem(at: destination)
            try FileManager.default.moveItem(at: temp, to: destination)
        })
    }

    public func install(_ agent: RegistryAgent,
                        platform: HostPlatform = .current) async throws
        -> InstalledAgentManifest {
        guard let method = agent.installMethod(platform: platform) else {
            throw AgentInstallError.unsupported
        }
        let fm = FileManager.default
        let staging = store.rootDirectory
            .appendingPathComponent("\(agent.id).staging", isDirectory: true)
        try? fm.removeItem(at: staging)
        try fm.createDirectory(at: staging, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: staging) }

        let final = store.agentDirectory(id: agent.id)
        let manifest: InstalledAgentManifest
        switch method {
        case .npx(let package, let args, let env):
            let result = try await shell.run(
                "npm install --prefix \(posixQuoted(staging.path)) \(package)",
                cwd: store.rootDirectory)
            guard result.exitCode == 0 else {
                throw AgentInstallError.commandFailed(output: result.output)
            }
            let binDir = staging.appendingPathComponent("node_modules/.bin")
            guard let binName = try resolveBinName(in: binDir, package: package) else {
                throw AgentInstallError.noExecutableFound
            }
            manifest = InstalledAgentManifest(
                id: agent.id, version: agent.version,
                executable: final.appendingPathComponent("node_modules/.bin/\(binName)").path,
                arguments: args, environment: env)
        case .binary(let archive, let cmd, let args, let env):
            let archiveFile = staging.appendingPathComponent(archive.lastPathComponent)
            try await download(archive, archiveFile)
            let extract = archive.lastPathComponent.hasSuffix(".zip")
                ? "ditto -x -k \(posixQuoted(archiveFile.path)) \(posixQuoted(staging.path))"
                : "tar -xzf \(posixQuoted(archiveFile.path)) -C \(posixQuoted(staging.path))"
            let result = try await shell.run(extract, cwd: staging)
            guard result.exitCode == 0 else {
                throw AgentInstallError.commandFailed(output: result.output)
            }
            try? fm.removeItem(at: archiveFile)
            let relative = cmd.hasPrefix("./") ? String(cmd.dropFirst(2)) : cmd
            guard fm.fileExists(atPath: staging.appendingPathComponent(relative).path) else {
                throw AgentInstallError.noExecutableFound
            }
            _ = try await shell.run(
                "chmod +x \(posixQuoted(staging.appendingPathComponent(relative).path))", cwd: staging)
            manifest = InstalledAgentManifest(
                id: agent.id, version: agent.version,
                executable: final.appendingPathComponent(relative).path,
                arguments: args, environment: env)
        }

        // Atomic-enough swap: remove old, move staging into place, write manifest.
        try? fm.removeItem(at: final)
        try fm.moveItem(at: staging, to: final)
        try store.write(manifest)
        return manifest
    }

    /// Picks the launchable entry in node_modules/.bin: single entry wins;
    /// otherwise the bin matching the package's own name (scope and version
    /// stripped) wins — dependency bins also land here (`codex` next to
    /// `codex-acp` from @openai/codex) and must never be preferred. Falls
    /// back to the longest entry contained in the package name.
    private func resolveBinName(in binDir: URL, package: String) throws -> String? {
        let entries = ((try? FileManager.default.contentsOfDirectory(
            at: binDir, includingPropertiesForKeys: nil)) ?? [])
            .map(\.lastPathComponent)
            .filter { !$0.hasPrefix(".") }
            .sorted()
        if entries.count == 1 { return entries[0] }
        let nameWithVersion = package.split(separator: "/").last.map(String.init) ?? package
        let baseName = nameWithVersion.firstIndex(of: "@")
            .map { String(nameWithVersion[..<$0]) } ?? nameWithVersion
        if entries.contains(baseName) { return baseName }
        return entries.filter { package.contains($0) }.max { $0.count < $1.count }
            ?? entries.first
    }
}
