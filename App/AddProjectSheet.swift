import SwiftUI
import TillerCore
import TillerGit
import AppKit
import Inject

private enum AddProjectStep {
    case menu
    case clone
    case create
}

struct AddProjectSheet: View {
    @ObserveInjection private var inject

    @Bindable var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var step: AddProjectStep = .menu
    @State private var errorMessage: String?

    var body: some View {
        Group {
            VStack(alignment: .leading, spacing: 0) {
                header
                switch step {
                case .menu: menuBody
                case .clone: CloneFromURLView(model: model, onDone: { dismiss() })
                case .create: CreateNewProjectView(model: model, onDone: { dismiss() })
                }
            }
            .padding(20)
            .frame(width: 460)
            .background(AppTheme.background)
        }
        .alert(
            "Git init failed",
            isPresented: .init(get: { errorMessage != nil }, set: { if !$0 { errorMessage = nil } })
        ) {
            Button("OK", role: .cancel) { errorMessage = nil }
        } message: {
            Text(errorMessage ?? "")
        }
    .enableInjection()
    }

    private var header: some View {
        HStack {
            if step == .menu {
                Text("Add a project")
                    .font(AppFont.system(size: 17, weight: .bold))
                    .foregroundStyle(AppTheme.title)
            } else {
                Button {
                    step = .menu
                } label: {
                    Label("Back", systemImage: "chevron.left")
                }
                .buttonStyle(.plain)
                .foregroundStyle(AppTheme.subtitle)
            }
            Spacer()
            Button {
                dismiss()
            } label: {
                Image(systemName: "xmark")
            }
            .buttonStyle(.plain)
            .foregroundStyle(AppTheme.subtitle)
        }
        .padding(.bottom, 16)
    }

    private var menuBody: some View {
        VStack(alignment: .leading, spacing: 8) {
            AddProjectMenuRow(
                icon: "folder", title: "Browse folder",
                subtitle: "Local project, Git repo, or folder with many repos",
                action: browseFolder
            )
            Text("OTHER WAYS TO ADD")
                .font(AppFont.system(size: 11, weight: .semibold))
                .foregroundStyle(AppTheme.meta)
                .padding(.top, 8)
            AddProjectMenuRow(
                icon: "globe", title: "Clone from URL",
                subtitle: "Clone a remote Git repository",
                action: { step = .clone }
            )
            AddProjectMenuRow(
                icon: "plus", title: "Create new project",
                subtitle: "Start from an empty folder",
                action: { step = .create }
            )
        }
    }

    private func browseFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.message = "Choose a project folder"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        if GitRepoDetection.isGitRepository(path: url.path) {
            Task {
                await model.addProject(at: url)
                dismiss()
            }
            return
        }
        let alert = NSAlert()
        alert.messageText = "Questa cartella non è un repository git"
        alert.informativeText =
            "Vuoi inizializzare un repository git in \(url.path)? "
            + "Senza git il progetto non avrà worktree né branch."
        alert.addButton(withTitle: "Inizializza git")
        alert.addButton(withTitle: "Aggiungi senza git")
        alert.addButton(withTitle: "Annulla")
        switch alert.runModal() {
        case .alertFirstButtonReturn:
            Task {
                do {
                    try await model.initGitAndAddProject(at: url)
                    dismiss()
                } catch {
                    errorMessage = "\(error)"
                }
            }
        case .alertSecondButtonReturn:
            Task {
                await model.addProject(at: url)
                dismiss()
            }
        default:
            break
        }
    }
}

private struct AddProjectMenuRow: View {
    @ObserveInjection private var inject

    let icon: String
    let title: String
    let subtitle: String
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: icon)
                .font(AppFont.system(size: 16))
                .foregroundStyle(AppTheme.title)
                .frame(width: 24)
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(AppFont.system(size: 13, weight: .semibold))
                    .foregroundStyle(AppTheme.title)
                Text(subtitle)
                    .font(AppFont.system(size: 11))
                    .foregroundStyle(AppTheme.subtitle)
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 10)
        .padding(.horizontal, 12)
        .contentShape(Rectangle())
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(hovering ? AppTheme.rowHover : AppTheme.primaryPillBg)
        )
        .onHover { hovering = $0 }
        .onTapGesture(perform: action)
    .enableInjection()
    }
}

private struct LocationRow: View {
    @ObserveInjection private var inject

    let parentDir: String
    let derivedPath: String
    let onPick: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text("Git repository in \(parentDir)")
                .font(AppFont.system(size: 12, weight: .semibold))
                .foregroundStyle(AppTheme.title)
            Text(derivedPath)
                .font(AppFont.system(size: 11))
                .foregroundStyle(AppTheme.meta)
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 7).fill(AppTheme.primaryPillBg))
        .contentShape(Rectangle())
        .onTapGesture(perform: onPick)
    .enableInjection()
    }
}

private func pickFolder() -> String? {
    let panel = NSOpenPanel()
    panel.canChooseDirectories = true
    panel.canChooseFiles = false
    panel.allowsMultipleSelection = false
    guard panel.runModal() == .OK else { return nil }
    return panel.url?.path
}

struct CloneFromURLView: View {
    @ObserveInjection private var inject

    @Bindable var model: AppModel
    let onDone: () -> Void

    @State private var url = ""
    @State private var parentDir = ProjectDefaults.defaultProjectsRoot()
    @State private var progress: Double?
    @State private var errorMessage: String?

    private var derivedPath: String {
        let name = url.isEmpty ? "project-name" : GitRemote.projectName(fromCloneURL: url)
        return (parentDir as NSString).appendingPathComponent(name)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Clone from URL")
                .font(AppFont.system(size: 17, weight: .bold))
                .foregroundStyle(AppTheme.title)
            Text("Clone a remote Git repository into a new project.")
                .font(AppFont.system(size: 12))
                .foregroundStyle(AppTheme.subtitle)

            VStack(alignment: .leading, spacing: 6) {
                Text("URL").font(AppFont.system(size: 12)).foregroundStyle(AppTheme.subtitle)
                TextField("https://github.com/owner/repo.git", text: $url)
                    .textFieldStyle(.plain)
                    .padding(10)
                    .background(RoundedRectangle(cornerRadius: 7).fill(AppTheme.filterFieldBg))
            }

            LocationRow(parentDir: parentDir, derivedPath: derivedPath) {
                if let picked = pickFolder() { parentDir = picked }
            }

            if let progress {
                ProgressView(value: progress)
            }

            Button("Clone project") { startClone() }
                .disabled(url.trimmingCharacters(in: .whitespaces).isEmpty || progress != nil)
                .frame(maxWidth: .infinity)
        }
        .alert(
            "Clone failed",
            isPresented: .init(get: { errorMessage != nil }, set: { if !$0 { errorMessage = nil } })
        ) {
            Button("OK", role: .cancel) { errorMessage = nil }
        } message: {
            Text(errorMessage ?? "")
        }
    .enableInjection()
    }

    private func startClone() {
        progress = 0
        Task {
            do {
                try await model.cloneProject(url: url, into: parentDir) { value in
                    Task { @MainActor in progress = value }
                }
                onDone()
            } catch {
                progress = nil
                errorMessage = "\(error)"
            }
        }
    }
}

struct CreateNewProjectView: View {
    @ObserveInjection private var inject

    @Bindable var model: AppModel
    let onDone: () -> Void

    @State private var name = ""
    @State private var parentDir = ProjectDefaults.defaultProjectsRoot()
    @State private var errorMessage: String?

    private var derivedPath: String {
        let folderName = name.isEmpty ? "my-project" : name
        return (parentDir as NSString).appendingPathComponent(folderName)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Create a new project")
                .font(AppFont.system(size: 17, weight: .bold))
                .foregroundStyle(AppTheme.title)
            Text("Name it and Tiller will create a real project with sensible defaults.")
                .font(AppFont.system(size: 12))
                .foregroundStyle(AppTheme.subtitle)

            VStack(alignment: .leading, spacing: 6) {
                Text("Name").font(AppFont.system(size: 12)).foregroundStyle(AppTheme.subtitle)
                TextField("my-project", text: $name)
                    .textFieldStyle(.plain)
                    .padding(10)
                    .background(RoundedRectangle(cornerRadius: 7).fill(AppTheme.filterFieldBg))
            }

            LocationRow(parentDir: parentDir, derivedPath: derivedPath) {
                if let picked = pickFolder() { parentDir = picked }
            }

            Button("Create project") { startCreate() }
                .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
                .frame(maxWidth: .infinity)
        }
        .alert(
            "Create failed",
            isPresented: .init(get: { errorMessage != nil }, set: { if !$0 { errorMessage = nil } })
        ) {
            Button("OK", role: .cancel) { errorMessage = nil }
        } message: {
            Text(errorMessage ?? "")
        }
    .enableInjection()
    }

    private func startCreate() {
        Task {
            do {
                try await model.createProject(name: name, in: parentDir)
                onDone()
            } catch {
                errorMessage = "\(error)"
            }
        }
    }
}
