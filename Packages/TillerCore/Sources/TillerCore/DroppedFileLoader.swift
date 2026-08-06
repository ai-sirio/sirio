import Foundation

/// Turns the `NSItemProvider`s a drop delivers into file URLs.
///
/// Foundation only, no UI framework: the chat lives in `App/` and the
/// terminal in `TillerTerminal`, and both need exactly this.
public enum DroppedFileLoader {
    /// Resolved one provider at a time on purpose. Resolution is concurrent
    /// per provider, and gathering the results in parallel would make the
    /// chips land in whatever order the callbacks happened to fire — the
    /// user's drag order is the order they expect to see.
    public static func urls(from providers: [NSItemProvider]) async -> [URL] {
        var result: [URL] = []
        for provider in providers {
            let url: URL? = await withCheckedContinuation { continuation in
                _ = provider.loadObject(ofClass: URL.self) { url, _ in
                    continuation.resume(returning: url)
                }
            }
            if let url { result.append(url) }
        }
        return result
    }

    /// Size for the image cap check. Anything unreadable reports 0, which
    /// classifies as a small file rather than silently dropping the item.
    public static func byteCount(of url: URL) -> Int {
        (try? url.resourceValues(forKeys: [.fileSizeKey]))?.fileSize ?? 0
    }
}
