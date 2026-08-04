// Tiller/App/AppTheme.swift
import SwiftUI
import AppKit
import TillerCore

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
    /// Central surface for the terminal main pane (#101112 dark, #F6F6F8 light).
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
    /// Central surface for the chat main pane (#101112 dark, #F6F6F8 light).
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
    /// Border and inset geometry for the central floating surface.
    static let mainSurfaceCornerRadius: CGFloat = 18
    static let mainSurfaceHorizontalInset: CGFloat = 10
    static let mainSurfaceVerticalInset: CGFloat = 12
    static let mainSurfaceShadowRadius: CGFloat = 18
    static let mainSurfaceShadowYOffset: CGFloat = 6
    static let mainSurfaceBorder = dynamic(
        light: NSColor.white.withAlphaComponent(0.72),
        dark: NSColor.white.withAlphaComponent(0.08))

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
        dark: NSColor(srgbRed: 0.85, green: 0.86, blue: 0.89, alpha: 1))
    static let titleSelected = dynamic(
        light: NSColor(srgbRed: 0.05, green: 0.05, blue: 0.08, alpha: 1),
        dark: .white)
    static let subtitle = dynamic(
        light: NSColor(srgbRed: 0.35, green: 0.37, blue: 0.45, alpha: 1),
        dark: NSColor(srgbRed: 0.72, green: 0.74, blue: 0.82, alpha: 1))
    /// Light value is darker than a naive mirror of the dark one: `meta` is
    /// caption-sized, and the previous #6B7085 cleared WCAG AA by 0.04.
    static let meta = dynamic(
        light: NSColor(srgbRed: 0.38, green: 0.40, blue: 0.48, alpha: 1),
        dark: NSColor(srgbRed: 0.66, green: 0.68, blue: 0.77, alpha: 1))
    static let primaryPillBg = dynamic(
        light: NSColor(srgbRed: 0.88, green: 0.885, blue: 0.92, alpha: 1),
        dark: NSColor(srgbRed: 0.200, green: 0.204, blue: 0.239, alpha: 1))
    static let filterFieldBg = dynamic(
        light: .white,
        dark: NSColor(srgbRed: 0.078, green: 0.082, blue: 0.106, alpha: 1))
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
        dark: NSColor(srgbRed: 0.157, green: 0.165, blue: 0.208, alpha: 1))
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

    /// Height of both bottom chrome bars: the sidebar footer and the usage bar.
    /// They sit in different `HSplitView` columns, so their top dividers line up
    /// only while the two bars are exactly as tall as each other. Sizing either
    /// one intrinsically instead lets them drift — the usage bar's 10pt font put
    /// it 19pt shorter than the sidebar footer, so the chat composer ran past the
    /// sidebar's divider.
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
        }
    }
}
