// Tiller/App/AppTheme.swift
import SwiftUI
import AppKit
import TillerCore
import Inject

/// Shared color tokens for Tiller's chrome, adaptive to the effective
/// appearance. `background` is the sidebar/chrome surface; `chatSurface` and
/// `terminalSurface` are the central-pane surface. The remaining tokens style
/// sidebar rows, labels, filter controls, hover, and selection. Agent accent
/// colors live in `AgentIcon`.
enum AppTheme {
    static let background = dynamic(
        light: NSColor(srgbRed: AppSurfaceColor.lightRed,
                       green: AppSurfaceColor.lightGreen,
                       blue: AppSurfaceColor.lightBlue,
                       alpha: 1),
        dark: NSColor(srgbRed: AppSurfaceColor.red, green: AppSurfaceColor.green, blue: AppSurfaceColor.blue, alpha: 1))
    /// The window canvas: the only surface reaching the window edges, and the
    /// one every card floats on.
    static let canvas = dynamic(
        light: NSColor(srgbRed: AppSurfaceColor.canvasLightRed,
                       green: AppSurfaceColor.canvasLightGreen,
                       blue: AppSurfaceColor.canvasLightBlue,
                       alpha: 1),
        dark: NSColor(srgbRed: AppSurfaceColor.canvasRed,
                      green: AppSurfaceColor.canvasGreen,
                      blue: AppSurfaceColor.canvasBlue,
                      alpha: 1))
    /// Central surface for the terminal main pane (#28292C dark, #F6F6F8 light).
    /// It matches `chatSurface` so terminal and chat panes share one surface.
    static let terminalSurface = dynamic(
        light: NSColor(srgbRed: AppSurfaceColor.chatLightRed,
                       green: AppSurfaceColor.chatLightGreen,
                       blue: AppSurfaceColor.chatLightBlue,
                       alpha: 1),
        dark: NSColor(srgbRed: AppSurfaceColor.terminalRed,
                      green: AppSurfaceColor.terminalGreen,
                      blue: AppSurfaceColor.terminalBlue,
                      alpha: 1))
    static let tabFocusAccent = dynamic(
        light: NSColor(srgbRed: 0.24, green: 0.38, blue: 0.78, alpha: 1),
        dark: NSColor(srgbRed: 0.55, green: 0.64, blue: 1.00, alpha: 1))
    static let tabNeedsInput = dynamic(
        light: NSColor(srgbRed: 0.67, green: 0.42, blue: 0.02, alpha: 1),
        dark: NSColor(srgbRed: 0.95, green: 0.72, blue: 0.28, alpha: 1))
    static let tabDone = dynamic(
        light: NSColor(srgbRed: 0.10, green: 0.45, blue: 0.22, alpha: 1),
        dark: NSColor(srgbRed: 0.48, green: 0.78, blue: 0.57, alpha: 1))
    static let tabError = dynamic(
        light: NSColor(srgbRed: 0.68, green: 0.12, blue: 0.17, alpha: 1),
        dark: NSColor(srgbRed: 0.94, green: 0.43, blue: 0.47, alpha: 1))
    /// Central surface for the chat main pane (#28292C dark, #F6F6F8 light).
    /// It matches `terminalSurface` so terminal and chat panes share one surface.
    static let chatSurface = dynamic(
        light: NSColor(srgbRed: AppSurfaceColor.chatLightRed,
                       green: AppSurfaceColor.chatLightGreen,
                       blue: AppSurfaceColor.chatLightBlue,
                       alpha: 1),
        dark: NSColor(srgbRed: AppSurfaceColor.chatRed,
                      green: AppSurfaceColor.chatGreen,
                      blue: AppSurfaceColor.chatBlue,
                      alpha: 1))
    /// Sidebar/tab bar/usage bar material tint. It intentionally matches
    /// `background` so both sidebars stay one chrome layer.
    static let chromeTint = background
    /// Geometry for the floating cards. Every panel — sidebar, central pane,
    /// right panel — is one of these, sitting on the canvas.
    static let cardCornerRadius: CGFloat = 6
    /// One value for the space between two cards, for the margin against the
    /// window's left and right edges, and for the margin below the split. A
    /// different outer margin would stop the canvas reading as a frame.
    static let cardGap: CGFloat = 10
    static let cardShadowRadius: CGFloat = 18
    static let cardShadowYOffset: CGFloat = 6
    /// The canvas band above the cards, holding the traffic lights and the
    /// chrome buttons. 28pt is the standard macOS titlebar height; it is
    /// deliberately thicker than `cardGap` because the window controls need it.
    static let titleStripHeight: CGFloat = 28
    /// Leading space in the title strip reserved for the traffic lights, which
    /// AppKit keeps drawing itself even under `.hiddenTitleBar`.
    static let trafficLightInset: CGFloat = 78
    /// Glyph size for the title-strip buttons. Matched to the traffic lights
    /// they sit beside — a default-sized SF Symbol reads noticeably larger
    /// than the 12pt window controls and breaks the row's rhythm.
    static let titleStripIconSize: CGFloat = 13
    static let titlebarControlFrame = CGSize(width: 24, height: 24)
    static let titlebarControlSpacing: CGFloat = 2

    /// Bottom edge of a pane tab chip. A chip's fill matches the chrome, so on
    /// the strip's material it has almost no edge of its own; this underlines
    /// it. Light against the dark chrome and dark against the light one — the
    /// point is contrast with the strip, not lightness for its own sake.
    static let tabChipUnderline = dynamic(
        light: NSColor(srgbRed: 0.62, green: 0.64, blue: 0.72, alpha: 1),
        dark: NSColor(srgbRed: 0.52, green: 0.55, blue: 0.64, alpha: 1))

    /// Shared border/divider stroke: sidebar dividers, settings field outlines,
    /// the update toast. Dark sits at ~1.9:1 against the chrome — readable as a
    /// boundary without turning every settings field into a hard-edged box.
    static let hairline = dynamic(
        light: NSColor(srgbRed: 0.82, green: 0.83, blue: 0.87, alpha: 1),
        dark: NSColor(srgbRed: 0.25, green: 0.26, blue: 0.31, alpha: 1))
    static let rowHover = dynamic(
        light: NSColor(srgbRed: 0.90, green: 0.905, blue: 0.93, alpha: 1),
        dark: NSColor(srgbRed: 0.125, green: 0.137, blue: 0.176, alpha: 1))
    static let selectionFill = dynamic(
        light: NSColor(srgbRed: 0.85, green: 0.86, blue: 0.91, alpha: 1),
        dark: NSColor(srgbRed: 0.169, green: 0.184, blue: 0.227, alpha: 1))
    static let selectionRing = dynamic(
        light: NSColor(srgbRed: 0.72, green: 0.74, blue: 0.82, alpha: 1),
        dark: NSColor(srgbRed: 0.227, green: 0.251, blue: 0.314, alpha: 1))
    static let title = dynamic(
        light: NSColor(srgbRed: 0.15, green: 0.16, blue: 0.20, alpha: 1),
        dark: NSColor(srgbRed: 242.0 / 255.0, green: 243.0 / 255.0, blue: 245.0 / 255.0, alpha: 1))
    static let titleSelected = dynamic(
        light: NSColor(srgbRed: 0.05, green: 0.05, blue: 0.08, alpha: 1),
        dark: NSColor(srgbRed: 242.0 / 255.0, green: 243.0 / 255.0, blue: 245.0 / 255.0, alpha: 1))
    static let subtitle = dynamic(
        light: NSColor(srgbRed: 0.35, green: 0.37, blue: 0.45, alpha: 1),
        dark: NSColor(srgbRed: 168.0 / 255.0, green: 171.0 / 255.0, blue: 178.0 / 255.0, alpha: 1))
    /// Light value is darker than a naive mirror of the dark one: `meta` is
    /// caption-sized, and the previous #6B7085 cleared WCAG AA by 0.04.
    static let meta = dynamic(
        light: NSColor(srgbRed: 0.38, green: 0.40, blue: 0.48, alpha: 1),
        dark: NSColor(srgbRed: 168.0 / 255.0, green: 171.0 / 255.0, blue: 178.0 / 255.0, alpha: 1))
    static let primaryPillBg = dynamic(
        light: NSColor(srgbRed: 0.88, green: 0.885, blue: 0.92, alpha: 1),
        dark: NSColor(srgbRed: 52.0 / 255.0, green: 53.0 / 255.0, blue: 57.0 / 255.0, alpha: 1))
    static let filterFieldBg = dynamic(
        light: .white,
        dark: NSColor(srgbRed: 52.0 / 255.0, green: 53.0 / 255.0, blue: 57.0 / 255.0, alpha: 1))
    /// Guide dell'albero in sidebar: abbastanza chiare da leggersi sul
    /// materiale traslucido, abbastanza tenui da non competere col testo.
    static let treeGuide = dynamic(
        light: NSColor.black.withAlphaComponent(0.12),
        dark: NSColor.white.withAlphaComponent(0.14))
    static let gitStaged = dynamic(
        light: NSColor(srgbRed: 0.08, green: 0.42, blue: 0.20, alpha: 1),
        dark: NSColor(srgbRed: 0.55, green: 0.82, blue: 0.63, alpha: 1))
    static let gitModified = dynamic(
        light: NSColor(srgbRed: 0.58, green: 0.35, blue: 0.05, alpha: 1),
        dark: NSColor(srgbRed: 0.91, green: 0.69, blue: 0.36, alpha: 1))
    static let gitUntracked = dynamic(
        light: NSColor(srgbRed: 0.08, green: 0.36, blue: 0.60, alpha: 1),
        dark: NSColor(srgbRed: 0.43, green: 0.68, blue: 0.91, alpha: 1))
    static let gitConflict = dynamic(
        light: NSColor(srgbRed: 0.62, green: 0.12, blue: 0.16, alpha: 1),
        dark: NSColor(srgbRed: 0.90, green: 0.58, blue: 0.60, alpha: 1))
    static let diffAddition = gitStaged
    static let diffAdditionBackground = dynamic(
        light: NSColor(srgbRed: 0.88, green: 0.96, blue: 0.90, alpha: 1),
        dark: NSColor(srgbRed: 0.08, green: 0.24, blue: 0.14, alpha: 1))
    static let diffDeletion = gitConflict
    static let diffDeletionBackground = dynamic(
        light: NSColor(srgbRed: 0.98, green: 0.89, blue: 0.90, alpha: 1),
        dark: NSColor(srgbRed: 0.27, green: 0.08, blue: 0.10, alpha: 1))
    static let diffHunkBackground = dynamic(
        light: NSColor(srgbRed: 0.88, green: 0.92, blue: 0.98, alpha: 1),
        dark: NSColor(srgbRed: 0.10, green: 0.17, blue: 0.28, alpha: 1))

    /// Chat card surface — one step off the chat background, no border. Every
    /// card in the transcript uses this instead of `.quaternary`, which
    /// bypasses the tuned light/dark pairs above.
    static let cardFill = dynamic(
        light: NSColor(srgbRed: 0.91, green: 0.915, blue: 0.94, alpha: 1),
        dark: NSColor(srgbRed: 52.0 / 255.0, green: 53.0 / 255.0, blue: 57.0 / 255.0, alpha: 1))
    /// Composer-only surface approved from the Codex-inspired visual review (#20232D).
    static let composerFill = Color(
        .sRGB,
        red: 32.0 / 255.0,
        green: 35.0 / 255.0,
        blue: 45.0 / 255.0,
        opacity: 1
    )
    enum ComposerAppearance {
        static let colorScheme: ColorScheme = .dark
        static let appKitAppearance = NSAppearance.Name.darkAqua
        static let primaryTextColor = NSColor.textColor
    }
    /// Clickable file paths in the transcript. `.tint` bypasses the tuned
    /// light/dark pairs and reads as a dark blue on the chat surface.
    static let fileLink = gitUntracked
    /// Left accent rails: the card's kind, readable while scrolling.
    static let railTask = dynamic(
        light: NSColor(srgbRed: 0.36, green: 0.30, blue: 0.68, alpha: 1),
        dark: NSColor(srgbRed: 0.49, green: 0.42, blue: 0.84, alpha: 1))
    static let railQuestion = gitModified
    static let railEdit = gitStaged
    static let railTool = dynamic(
        light: NSColor(srgbRed: 0.55, green: 0.57, blue: 0.65, alpha: 1),
        dark: NSColor(srgbRed: 0.40, green: 0.42, blue: 0.50, alpha: 1))

    /// Height of the usage bar, the app's single bottom chrome bar. It spans the
    /// full window below the split, so nothing has to line up with it any more —
    /// the sidebar footer that used to sit beside it in another `HSplitView`
    /// column is gone, and with it the invariant that the two stay equally tall.
    /// Still fixed rather than intrinsic: the bar's 10pt font would size it 19pt
    /// shorter, and its segments truncate on a narrow window instead of wrapping.
    static let bottomBarHeight: CGFloat = 30

    /// Resolves at draw time against the view's effective appearance — the
    /// same mechanism behind Apple's semantic colors.
    private static func dynamic(light: NSColor, dark: NSColor) -> Color {
        Color(nsColor: NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? dark : light
        })
    }
}

/// Hover feedback for the small chrome icon buttons (close tab, settings,
/// refresh): a rounded backplate that reads on any backdrop — including an
/// already-hovered sidebar row — plus a slight press scale for instant
/// touch-down response.
struct HoverIconButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        HoverIconLabel(configuration: configuration)
    }

    private struct HoverIconLabel: View {
        @ObserveInjection private var inject

        let configuration: ButtonStyle.Configuration
        @State private var hovering = false

        var body: some View {
            configuration.label
                .padding(3)
                .background(
                    RoundedRectangle(cornerRadius: 4)
                        .fill(Color.primary.opacity(hovering ? 0.1 : 0))
                )
                .scaleEffect(configuration.isPressed ? 0.92 : 1)
                .onHover { hovering = $0 }
        .enableInjection()
        }
    }
}
