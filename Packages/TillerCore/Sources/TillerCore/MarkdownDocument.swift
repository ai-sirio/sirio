import Foundation
import Observation

/// Buffer di editing di un file markdown aperto in una tab. Salvataggio
/// manuale (⌘S); un watcher sul file rileva modifiche esterne (agenti,
/// git): buffer pulito → reload automatico, buffer sporco → conflitto
/// che l'utente risolve dal banner (Ricarica / Mantieni).
@Observable @MainActor
public final class MarkdownDocument {
    public let fileURL: URL
    public var text: String
    public private(set) var savedText: String
    public private(set) var externalChangeConflict = false
    public private(set) var fileDeleted = false

    public var isDirty: Bool { text != savedText }

    @ObservationIgnored private var watcher: FileWatcher?

    public init(fileURL: URL) throws {
        self.fileURL = fileURL
        let content = try String(contentsOf: fileURL, encoding: .utf8)
        self.text = content
        self.savedText = content
        startWatching()
    }

    public func save() throws {
        try text.write(to: fileURL, atomically: true, encoding: .utf8)
        savedText = text
        fileDeleted = false
        externalChangeConflict = false
        // Il write atomico sostituisce l'inode: il fd osservato è orfano.
        startWatching()
    }

    public func reloadFromDisk() {
        guard let disk = try? String(contentsOf: fileURL, encoding: .utf8) else { return }
        text = disk
        savedText = disk
        externalChangeConflict = false
    }

    /// "Mantieni": il buffer resta; savedText si allinea al disco così il
    /// prossimo ⌘S sovrascrive consapevolmente la versione esterna.
    public func keepLocalBuffer() {
        if let disk = try? String(contentsOf: fileURL, encoding: .utf8) {
            savedText = disk
        }
        externalChangeConflict = false
    }

    public func stopWatching() {
        watcher?.cancel()
        watcher = nil
    }

    // MARK: - Watcher

    private func startWatching() {
        watcher?.cancel()
        watcher = FileWatcher(path: fileURL.path) { [weak self] event in
            let rawValue = event.rawValue
            Task { @MainActor [weak self] in
                self?.handleEvent(DispatchSource.FileSystemEvent(rawValue: rawValue))
            }
        }
    }

    private func handleEvent(_ event: DispatchSource.FileSystemEvent) {
        if event.contains(.delete) || event.contains(.rename) {
            // Write atomico o git checkout: il file è stato sostituito
            // (esiste un nuovo inode) oppure davvero cancellato.
            if FileManager.default.fileExists(atPath: fileURL.path) {
                startWatching()
                handleExternalChange()
            } else {
                handleFileGone()
            }
        } else if event.contains(.write) {
            handleExternalChange()
        }
    }

    /// Internal (non private) per testare la logica senza dipendere dal
    /// timing degli eventi DispatchSource.
    func handleExternalChange() {
        guard let disk = try? String(contentsOf: fileURL, encoding: .utf8) else { return }
        guard disk != savedText else { return }   // eco del nostro save
        if isDirty {
            externalChangeConflict = true
        } else {
            text = disk
            savedText = disk
        }
    }

    func handleFileGone() {
        stopWatching()
        fileDeleted = true
    }
}

/// Wrapper non-actor attorno a DispatchSourceFileSystemObject: il cancel
/// in deinit non può vivere su un tipo @MainActor (deinit non isolato).
final class FileWatcher: @unchecked Sendable {
    private let source: DispatchSourceFileSystemObject

    init?(path: String, onEvent: @escaping @Sendable (DispatchSource.FileSystemEvent) -> Void) {
        let fd = open(path, O_EVTONLY)
        guard fd >= 0 else { return nil }
        let source = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: fd, eventMask: [.write, .delete, .rename], queue: .main
        )
        source.setEventHandler { onEvent(source.data) }
        source.setCancelHandler { close(fd) }
        source.resume()
        self.source = source
    }

    func cancel() { source.cancel() }

    deinit {
        // cancel è idempotente: sicuro anche se già chiamato esplicitamente.
        source.cancel()
    }
}
