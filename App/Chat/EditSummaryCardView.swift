import SwiftUI
import TillerACP
import TillerCore
import TillerGit

/// Card di fine turno "N file modificati": ogni riga apre il diff nel right
/// panel; "Ripristina" scarta le modifiche del file via git (con conferma).
/// Visibilità post-hoc + undo — il permission gate resta la difesa preventiva.
struct EditSummaryCardView: View {
    let paths: [String]
    let worktree: Worktree
    let appModel: AppModel

    @State private var confirmingPath: String?
    @State private var revertError: String?
    @State private var revertedPaths: Set<String> = []
    @State private var revertInProgress = false

    private var isGitProject: Bool {
        appModel.isGitProject(id: worktree.projectId)
    }

    var body: some View {
        ChatCard(kind: .edit) {
            VStack(alignment: .leading, spacing: 6) {
                Label(paths.count == 1 ? "1 file changed"
                                       : "\(paths.count) files changed",
                      systemImage: "pencil.line")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                ForEach(paths, id: \.self) { path in
                    row(path)
                }
                if let revertError {
                    Text(revertError)
                        .font(.caption)
                        .foregroundStyle(.red)
                }
            }
        }
        .confirmationDialog(
            "Revert \((confirmingPath as NSString?)?.lastPathComponent ?? "")?",
            isPresented: Binding(
                get: { confirmingPath != nil },
                set: { if !$0 { confirmingPath = nil } })) {
            Button("Revert", role: .destructive) {
                if let path = confirmingPath { revert(path) }
                confirmingPath = nil
            }
            Button("Cancel", role: .cancel) { confirmingPath = nil }
        } message: {
            Text("The file's changes will be lost (git restore / clean).")
        }
    }

    private func row(_ path: String) -> some View {
        HStack(spacing: 8) {
            Button {
                appModel.requestChatFollow(path: path, worktreeId: worktree.id)
            } label: {
                Label((path as NSString).lastPathComponent,
                      systemImage: "arrow.up.forward.square")
                    .font(.caption)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.tint)
            Button {
                appModel.openFileReference(path, in: worktree)
            } label: {
                Image(systemName: "chevron.left.forwardslash.chevron.right")
            }
            .buttonStyle(.plain)
            .help("Open in editor")
            Spacer()
            if revertedPaths.contains(path) {
                Text("reverted")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            } else if isGitProject {
                Button("Revert") { confirmingPath = path }
                    .controlSize(.small)
                    .disabled(revertInProgress)
            }
        }
    }

    private func revert(_ path: String) {
        revertInProgress = true
        revertError = nil
        Task {
            defer { revertInProgress = false }
            let root = URL(fileURLWithPath: worktree.path).standardizedFileURL.path
            let relative = path.hasPrefix(root + "/")
                ? String(path.dropFirst(root.count + 1))
                : path
            do {
                let status = try await GitStatus.load(in: worktree.path)
                guard let entry = status.entries.first(where: {
                    $0.path.value == relative
                }) else {
                    revertError = "No changes to revert for \(relative)."
                    return
                }
                if entry.isUntracked {
                    try await GitActions.discardUntracked([entry], in: worktree.path)
                } else {
                    try await GitActions.discardChanges([entry], in: worktree.path)
                }
                revertedPaths.insert(path)
            } catch {
                revertError = error.localizedDescription
            }
        }
    }
}
