// Tiller/App/AppTheme.swift
import SwiftUI
import TillerCore

/// Shared color tokens for Tiller's dark chrome. `background` is the opaque
/// main-pane and terminal surface, and it tints the native sidebar material
/// through `SidebarMaterialContainer`. The remaining tokens style sidebar
/// rows, labels, filter controls, hover, and selection. Agent accent colors
/// live in `AgentIcon`.
enum AppTheme {
    static let background      = Color(red: AppSurfaceColor.red, green: AppSurfaceColor.green, blue: AppSurfaceColor.blue)
    /// Indigo tint for the sidebar/tab bar/usage bar material (opencode-style).
    static let chromeTint      = Color(red: 0.110, green: 0.110, blue: 0.176)
    static let hairline       = Color(red: 0.165, green: 0.176, blue: 0.220)
    static let rowHover       = Color(red: 0.125, green: 0.137, blue: 0.176)
    static let selectionFill  = Color(red: 0.169, green: 0.184, blue: 0.227)
    static let selectionRing  = Color(red: 0.227, green: 0.251, blue: 0.314)
    static let title          = Color(red: 0.85, green: 0.86, blue: 0.89)
    static let titleSelected  = Color.white
    static let subtitle       = Color(red: 0.72, green: 0.74, blue: 0.82)
    static let meta           = Color(red: 0.66, green: 0.68, blue: 0.77)
    static let primaryPillBg  = Color(red: 0.200, green: 0.204, blue: 0.239)
    static let filterFieldBg  = Color(red: 0.078, green: 0.082, blue: 0.106)
}
