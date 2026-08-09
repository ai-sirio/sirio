import SwiftUI
import Inject

/// Shown when a worktree has zero tabs — no terminals, no PTYs.
/// The user can create a new terminal tab via the button or ⌘T.
struct EmptyWorktreeView: View {
    @ObserveInjection private var inject

    let onNewTerminal: () -> Void

    var body: some View {
        VStack(spacing: 16) {
            Image(systemName: "terminal")
                .font(AppFont.system(size: 48))
                .foregroundStyle(.secondary)

            Text("No Terminals")
                .font(AppFont.title2)
                .foregroundStyle(.primary)

            Text("Open a new terminal to get started.")
                .font(AppFont.subheadline)
                .foregroundStyle(.secondary)

            Button("New Terminal", action: onNewTerminal)
                .buttonStyle(.borderedProminent)
                .controlSize(.large)

            Text("⌘T")
                .font(AppFont.caption)
                .foregroundStyle(.tertiary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    .enableInjection()
    }
}

#Preview {
    EmptyWorktreeView(onNewTerminal: {})
}
