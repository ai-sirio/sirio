import MarkdownUI
import SwiftUI

extension Theme {
    /// Compact markdown theme for the chat transcript, sized from the
    /// interface font setting (the stock .gitHub theme renders ~16px web-page
    /// typography that clashes with native panes). Prose takes the system UI
    /// font; only fenced code drops to SF Mono Light, as Xcode does.
    ///
    /// Cached per size rather than rebuilt per access: this is read on every
    /// transcript render, and only 11 sizes exist.
    @MainActor
    private static var themeCache: [CGFloat: Theme] = [:]

    @MainActor
    static var tiller: Theme {
        let base = AppFont.base
        if let cached = themeCache[base] { return cached }
        let theme = build()
        themeCache[base] = theme
        return theme
    }

    @MainActor
    private static func build() -> Theme {
        Theme.gitHub
        .text {
            FontSize(AppFont.scaled(13))
        }
        .code {
            FontFamilyVariant(.monospaced)
            FontWeight(AppFont.codeWeight)
            FontSize(AppFont.scaled(AppFont.codeSize))
        }
        .heading1 { configuration in
            configuration.label
                .markdownMargin(top: 12, bottom: 4)
                .markdownTextStyle {
                    FontWeight(.semibold)
                    FontSize(AppFont.scaled(15))
                }
        }
        .heading2 { configuration in
            configuration.label
                .markdownMargin(top: 10, bottom: 4)
                .markdownTextStyle {
                    FontWeight(.semibold)
                    FontSize(AppFont.scaled(14))
                }
        }
        .heading3 { configuration in
            configuration.label
                .markdownMargin(top: 8, bottom: 2)
                .markdownTextStyle {
                    FontWeight(.semibold)
                    FontSize(AppFont.scaled(13))
                }
        }
    }
}
