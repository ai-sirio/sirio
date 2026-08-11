import Darwin
import Foundation

/// Reads `handle` until EOF or `timeoutSeconds` have elapsed since the call started, whichever
/// comes first. `FileHandle.readDataToEndOfFile()` has no such bound: when the process on the
/// other end of the pipe hangs before writing/closing (observed with Claude Code's Stop-hook
/// runner), that call blocks forever and wedges the hook that spawned tillerctl.
func readBoundedStdin(_ handle: FileHandle = .standardInput, timeoutSeconds: TimeInterval = 5) -> Data {
    let fd = handle.fileDescriptor
    var data = Data()
    var buffer = [UInt8](repeating: 0, count: 4096)
    let deadline = Date().addingTimeInterval(timeoutSeconds)

    while true {
        let remainingMs = deadline.timeIntervalSinceNow * 1000
        guard remainingMs > 0 else { break }

        var fds = pollfd(fd: fd, events: Int16(POLLIN), revents: 0)
        let ready = poll(&fds, 1, Int32(remainingMs))
        guard ready > 0, fds.revents & Int16(POLLIN) != 0 else { break }

        let bytesRead = buffer.withUnsafeMutableBytes { raw in
            read(fd, raw.baseAddress, raw.count)
        }
        guard bytesRead > 0 else { break }
        data.append(buffer, count: bytesRead)
    }
    return data
}
