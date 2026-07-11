import AppKit

/// Il bottone rosso nasconde la finestra invece di chiuderla. Chiuderla
/// davvero smonterebbe la gerarchia SwiftUI: ogni PtyTerminalPane riceve
/// onDisappear, ferma il PTY e azzera lo stato agenti — svuotando il roster
/// della menu bar proprio nello scenario per cui esiste (agenti vivi con
/// app ridotta a icona). `orderOut` toglie la finestra dallo schermo ma le
/// view restano montate, quindi PTY e status pipeline continuano a girare.
///
/// Proxy attorno al delegate che SwiftUI installa sulla finestra: inoltra
/// tutto all'originale e intercetta solo `windowShouldClose`.
final class HideOnCloseWindowDelegate: NSObject, NSWindowDelegate {
    private let original: NSWindowDelegate?

    init(wrapping original: NSWindowDelegate?) {
        self.original = original
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        sender.orderOut(nil)
        return false
    }

    override func responds(to aSelector: Selector!) -> Bool {
        super.responds(to: aSelector) || (original?.responds(to: aSelector) ?? false)
    }

    override func forwardingTarget(for aSelector: Selector!) -> Any? {
        if original?.responds(to: aSelector) == true { return original }
        return super.forwardingTarget(for: aSelector)
    }
}

/// Mai letta né scritta: serve solo come indirizzo stabile per la chiave
/// di objc_setAssociatedObject, quindi l'accesso concorrente è innocuo.
private nonisolated(unsafe) var hideOnCloseProxyKey: UInt8 = 0

/// Riferimento alla main window catturato all'installazione del proxy.
/// Serve a riaprirla dopo hide-on-close: una finestra ordered-out risponde
/// `false` sia a `isVisible` sia a `canBecomeMain`, quindi nessun predicato
/// su `NSApp.windows` può distinguerla in modo affidabile da popover della
/// menu bar o finestre Sparkle — il riferimento diretto sì.
@MainActor
final class MainWindowRef {
    static let shared = MainWindowRef()
    weak var window: NSWindow?
}

extension NSWindow {
    /// Installa il proxy una sola volta. `NSWindow.delegate` è weak, quindi
    /// il proxy viene retained via associated object legato alla finestra.
    func installHideOnClose() {
        MainWindowRef.shared.window = self
        guard !(delegate is HideOnCloseWindowDelegate) else { return }
        let proxy = HideOnCloseWindowDelegate(wrapping: delegate)
        objc_setAssociatedObject(self, &hideOnCloseProxyKey, proxy, .OBJC_ASSOCIATION_RETAIN)
        delegate = proxy
    }
}
