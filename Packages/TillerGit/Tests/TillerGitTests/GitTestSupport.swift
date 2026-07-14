import Foundation

@discardableResult
func runGitForTest(_ arguments: [String], in directory: URL) throws -> String {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
    process.arguments = arguments
    process.currentDirectoryURL = directory
    let stdout = Pipe()
    let stderr = Pipe()
    process.standardOutput = stdout
    process.standardError = stderr
    try process.run()
    process.waitUntilExit()
    let output = String(decoding: stdout.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
    if process.terminationStatus != 0 {
        let error = String(decoding: stderr.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
        throw NSError(domain: "GitTest", code: Int(process.terminationStatus),
                      userInfo: [NSLocalizedDescriptionKey: error])
    }
    return output
}

func makeGitTestRepository(withCommit: Bool = true) throws -> URL {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("tiller-git-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    try runGitForTest(["init", "-b", "main"], in: root)
    try runGitForTest(["config", "user.email", "test@tiller.dev"], in: root)
    try runGitForTest(["config", "user.name", "Tiller Test"], in: root)
    if withCommit {
        try "one\n".write(to: root.appendingPathComponent("file.txt"), atomically: true, encoding: .utf8)
        try runGitForTest(["add", "--", "file.txt"], in: root)
        try runGitForTest(["commit", "-m", "root"], in: root)
    }
    return root
}
