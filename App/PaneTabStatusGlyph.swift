import SwiftUI
import TillerCore
import Inject

enum PaneTabStatusGlyphKind: Equatable {
    case none, running, needsInput, done, error

    static func forStatus(_ status: AgentStatus?) -> Self {
        switch status {
        case nil: .none
        case .running: .running
        case .needsInput: .needsInput
        case .done: .done
        case .error: .error
        }
    }
}

struct PaneTabStatusGlyph: View {
    @ObserveInjection private var inject

    let status: AgentStatus?
    let agentID: String?

    var body: some View {
        Group {
            switch PaneTabStatusGlyphKind.forStatus(status) {
            case .none:
                Color.clear
            case .running:
                RunningDots(color: AgentIcon.color(for: agentID ?? ""), dotSize: 3)
            case .needsInput:
                Image(systemName: "exclamationmark")
                    .font(AppFont.system(size: 8, weight: .bold))
                    .foregroundStyle(AppTheme.tabNeedsInput)
            case .done:
                Image(systemName: "checkmark")
                    .font(AppFont.system(size: 8, weight: .bold))
                    .foregroundStyle(AppTheme.tabDone)
            case .error:
                Image(systemName: "exclamationmark.triangle.fill")
                    .font(AppFont.system(size: 8, weight: .semibold))
                    .foregroundStyle(AppTheme.tabError)
            }
        }
        .frame(width: 14, height: 14)
        .help(status?.humanLabel ?? "")
        .accessibilityLabel(status?.humanLabel ?? "No agent activity")
        .accessibilityHidden(status == nil)
    .enableInjection()
    }
}
