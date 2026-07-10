// Tiller/App/AppFont.swift
import SwiftUI

/// Shared monospaced typography for the app's opencode-style chrome.
/// Mirrors `AppTheme`'s single-source-of-truth pattern: one place to change
/// font family/design, every call site inherits it automatically.
enum AppFont {
    static func system(size: CGFloat, weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight, design: .monospaced)
    }
}
