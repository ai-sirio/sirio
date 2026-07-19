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
        VStack(alignment: .leading, spacing: 6) {
            Label(paths.count == 1 ? "1 file modificato"
                                   : "\(paths.count) file modificati",
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
        .padding(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.quaternary.opacity(0.4), in: RoundedRectangle(cornerRadius: 8))
        .confirmationDialog(
            "Ripristinare \((confirmingPath as NSString?)?.lastPathComponent ?? "")?",
            isPresented: Binding(
                get: { confirmingPath != nil },
                set: { if !$0 { confirmingPath = nil } })) {
            Button("Ripristina", role: .destructive) {
                if let path = confirmingPath { revert(path) }
                confirmingPath = nil
            }
            Button("Annulla", role: .cancel) { confirmingPath = nil }
        } message: {
            Text("Le modifiche del file andranno perse (git restore / clean).")
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
            Spacer()
            if revertedPaths.contains(path) {
                Text("ripristinato")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            } else if isGitProject {
                Button("Ripristina") { confirmingPath = path }
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
                    revertError = "Nessuna modifica da ripristinare per \(relative)."
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
