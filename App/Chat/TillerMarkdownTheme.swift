import MarkdownUI
import SwiftUI

extension Theme {
    /// Compact system-font markdown theme for the chat transcript: 13pt body
    /// aligned with the rest of the Tiller UI (the stock .gitHub theme
    /// renders ~16px web-page typography that clashes with native panes).
    @MainActor
    static let tiller = Theme.gitHub
        .text {
            FontSize(13)
        }
        .code {
            FontFamilyVariant(.monospaced)
            FontSize(12)
        }
        .heading1 { configuration in
            configuration.label
                .markdownMargin(top: 12, bottom: 4)
                .markdownTextStyle {
                    FontWeight(.semibold)
                    FontSize(15)
                }
        }
        .heading2 { configuration in
            configuration.label
                .markdownMargin(top: 10, bottom: 4)
                .markdownTextStyle {
                    FontWeight(.semibold)
                    FontSize(14)
                }
        }
        .heading3 { configuration in
            configuration.label
                .markdownMargin(top: 8, bottom: 2)
                .markdownTextStyle {
                    FontWeight(.semibold)
                    FontSize(13)
                }
        }
}
