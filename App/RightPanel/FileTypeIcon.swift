import SwiftUI
import TillerCore
import Inject

/// File-type glyph shared by the file explorer and the changes list. The theme
/// resolves to either an SF Symbol name or an asset-catalog image.
struct FileTypeIcon: View {
    @ObserveInjection private var inject

    let key: FileIconKey
    let theme: FileIconTheme
    var tint: Color = AppTheme.subtitle

    var body: some View {
        Group {
            switch theme.iconRef(for: key) {
            case .system(let name):
                Image(systemName: name)
                    .foregroundStyle(tint)
            case .asset(let name):
                Image(name)
                    .resizable()
                    .scaledToFit()
                    .frame(width: 14, height: 14)
            }
        }
    .enableInjection()
    }
}
