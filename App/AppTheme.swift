// Tiller/App/AppTheme.swift
import SwiftUI
import AppKit
import TillerCore

/// Shared color tokens for Tiller's chrome, adaptive to the effective
/// appearance (dark values are the original palette; light is a hand-tuned
/// cool-gray mirror with a subtle indigo tint). `background` is the opaque
/// main-pane and terminal surface, and it tints the native sidebar material
/// through `SidebarMaterialContainer`. The remaining tokens style sidebar
/// rows, labels, filter controls, hover, and selection. Agent accent colors
/// live in `AgentIcon`.
enum AppTheme {
    static let background = dynamic(
        light: NSColor(srgbRed: 0.965, green: 0.965, blue: 0.975, alpha: 1),
        dark: NSColor(srgbRed: AppSurfaceColor.red, green: AppSurfaceColor.green, blue: AppSurfaceColor.blue, alpha: 1))
    /// Neutral near-black tint for the sidebar/tab bar/usage bar material.
    static let chromeTint = dynamic(
        light: NSColor(srgbRed: 0.92, green: 0.92, blue: 0.96, alpha: 1),
        dark: NSColor(srgbRed: 0.075, green: 0.075, blue: 0.075, alpha: 1))
    static let hairline = dynamic(
        light: NSColor(srgbRed: 0.82, green: 0.83, blue: 0.87, alpha: 1),
        dark: NSColor(srgbRed: 0.165, green: 0.176, blue: 0.220, alpha: 1))
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
    static let meta = dynamic(
        light: NSColor(srgbRed: 0.42, green: 0.44, blue: 0.52, alpha: 1),
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
