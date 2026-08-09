import AppKit
import SwiftUI
import TillerCore

/// The interface type scale, in one place.
///
/// Every `.font(...)` in the app resolves through `AppFont`, so the family
/// (SF Mono) and the base size live at a single point instead of being
/// re-decided at 260 call sites.
///
/// Reactivity comes from `@Observable`: a view body that asks for
/// `AppFont.caption` reads `UIFontScale.shared.size`, which registers the
/// dependency, so changing the size in Settings re-renders that view. No
/// per-view `@Environment` and — deliberately — no `.id()` on the root, which
/// would remount the terminal hosts and kill their PTYs.
@MainActor
@Observable
final class UIFontScale {
    static let shared = UIFontScale()

    var size: CGFloat

    private init() {
        let stored = UserDefaults.standard.object(forKey: AppSettings.uiFontSizeKey) as? Int
        size = CGFloat(AppSettings.clampUIFontSize(stored ?? AppSettings.defaultUIFontSize))
    }

    func apply(_ points: Int) {
        size = CGFloat(AppSettings.clampUIFontSize(points))
    }
}

@MainActor
enum AppFont {
    /// Base interface size, 13pt unless Settings says otherwise.
    static var base: CGFloat { UIFontScale.shared.size }

    /// Fixed sizes in the codebase encode a deliberate micro-hierarchy (10pt
    /// badges under 13pt titles). Shifting them by the same delta keeps that
    /// hierarchy intact instead of collapsing everything onto one size.
    private static var delta: CGFloat { base - CGFloat(AppSettings.defaultUIFontSize) }

    static func scaled(_ points: CGFloat) -> CGFloat { max(6, points + delta) }

    // MARK: - Two scales, as Xcode has them

    /// Xcode's editor is SF Mono Light 12; its menus, navigator and settings
    /// are the plain system UI font. Tiller follows the same split: chrome,
    /// prose and the composer go through `system`, code surfaces (editor,
    /// diffs, highlighted blocks) through `mono`. The terminal is neither —
    /// it runs a Nerd Font, see `TillerTerminalTheme`.
    ///
    /// "SF Mono" is unreachable by name: `NSFont(name: "SF Mono")` is nil and
    /// `CTFontCreateWithName("SF Mono")` answers *Helvetica* instead of
    /// failing. The system monospaced font is the same typeface and the only
    /// reliable way to ask for it.
    static let codeSize: CGFloat = 12
    static let codeWeight: Font.Weight = .light
    static let codeNSWeight: NSFont.Weight = .light

    static func system(size: CGFloat, weight: Font.Weight? = nil,
                       design: Font.Design? = nil) -> Font {
        Font.system(size: scaled(size), weight: weight ?? .regular,
                    design: design ?? .default)
    }

    static func mono(size: CGFloat = codeSize, weight: Font.Weight? = nil) -> Font {
        Font.system(size: scaled(size), weight: weight ?? codeWeight,
                    design: .monospaced)
    }

    static func system(_ style: Font.TextStyle, design: Font.Design? = nil,
                       weight: Font.Weight? = nil) -> Font {
        system(size: points(for: style), weight: weight, design: design)
    }

    /// Anchored to `base` rather than to AppKit's own text styles, so one
    /// stepper moves the whole scale.
    ///
    /// The steps below `base` are deliberately tight: the stock macOS spread
    /// (caption at −3, caption2 at −4) would put the two most-used tokens at
    /// 10 and 9pt. `caption` lands on 11pt instead.
    private static func points(for style: Font.TextStyle) -> CGFloat {
        switch style {
        case .largeTitle: base + 10
        case .title: base + 7
        case .title2: base + 3
        case .title3: base + 1
        case .headline, .body: base
        case .callout, .subheadline: base - 1
        case .footnote, .caption: base - 2
        case .caption2: base - 3
        @unknown default: base
        }
    }

    static var largeTitle: Font { system(.largeTitle) }
    static var title: Font { system(.title) }
    static var title2: Font { system(.title2) }
    static var title3: Font { system(.title3) }
    static var headline: Font { system(.headline, weight: .semibold) }
    static var body: Font { system(.body) }
    static var callout: Font { system(.callout) }
    static var subheadline: Font { system(.subheadline) }
    static var footnote: Font { system(.footnote) }
    static var caption: Font { system(.caption) }
    static var caption2: Font { system(.caption2) }

    /// AppKit bridge for the text views SwiftUI's environment cannot reach —
    /// the composer, the markdown transcript renderer, the code editor.
    static func nsFont(size: CGFloat, weight: NSFont.Weight = .regular) -> NSFont {
        .systemFont(ofSize: scaled(size), weight: weight)
    }

    static func nsMono(size: CGFloat = codeSize,
                       weight: NSFont.Weight = codeNSWeight) -> NSFont {
        .monospacedSystemFont(ofSize: scaled(size), weight: weight)
    }

    static var nsBody: NSFont { .systemFont(ofSize: base, weight: .regular) }
}
