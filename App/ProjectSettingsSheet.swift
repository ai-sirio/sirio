import SwiftUI
import TillerCore
import TillerGit
import AppKit
import Foundation
import UniformTypeIdentifiers
import Inject

/// Sheet for editing a single project's sidebar identity, icon, and worktree
/// defaults. Opened from the gear icon on `ProjectRow`.
struct ProjectSettingsSheet: View {
    @ObserveInjection private var inject

    @Bindable var model: AppModel
    let project: Project
    @Environment(\.dismiss) private var dismiss

    @State private var showDeleteConfirm = false

    /// Re-reads the project from the live model on every render, so edits
    /// made through `AppModel` (which patches `projects` in place) show up
    /// immediately instead of being stuck on the sheet's opening snapshot.
    private var currentProject: Project {
        model.projects.first(where: { $0.id == project.id }) ?? project
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                header
                IdentitySection(model: model, project: currentProject)
                Divider().overlay(AppTheme.hairline)
                RepoIconSection(model: model, project: currentProject)
                if model.isGitProject(currentProject) {
                    Divider().overlay(AppTheme.hairline)
                    WorktreeBaseSection(model: model, project: currentProject)
                    Divider().overlay(AppTheme.hairline)
                    WorktreeLocationSection(model: model, project: currentProject)
                }
            }
            .padding(20)
        }
        .frame(width: 600, height: 500)
        .background(AppTheme.background)
        .alert("Delete \(currentProject.name)?", isPresented: $showDeleteConfirm) {
            Button("Delete", role: .destructive) {
                Task {
                    await model.removeProject(currentProject)
                    dismiss()
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This removes the project and all its worktrees from Tiller. Files on disk are not deleted.")
        }
    .enableInjection()
    }

    private var header: some View {
        HStack {
            Button("Close") { dismiss() }
                .buttonStyle(.plain)
                .foregroundStyle(AppTheme.subtitle)
            Spacer()
            Button {
                showDeleteConfirm = true
            } label: {
                Image(systemName: "trash")
            }
            .buttonStyle(.plain)
            .foregroundStyle(AppTheme.subtitle)
        }
    }
}

private struct IdentitySection: View {
    @ObserveInjection private var inject

    @Bindable var model: AppModel
    let project: Project

    private var displayNameBinding: Binding<String> {
        Binding(
            get: { project.displayName ?? "" },
            set: { newValue in
                let trimmed = newValue.trimmingCharacters(in: .whitespaces)
                Task { await model.setProjectDisplayName(project, displayName: trimmed.isEmpty ? nil : trimmed) }
            }
        )
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Identity").font(.system(size: 15, weight: .bold)).foregroundStyle(AppTheme.title)
            Text("How this project is labeled in the sidebar and tabs — doesn't rename the folder on disk.")
                .font(.system(size: 12)).foregroundStyle(AppTheme.subtitle)

            Text("Repository Type").font(.system(size: 13, weight: .semibold)).foregroundStyle(AppTheme.title)
            Text(model.isGitProject(project) ? "Git" : "Folder")
                .font(.system(size: 12)).foregroundStyle(AppTheme.meta)

            Text("Display Name").font(.system(size: 13, weight: .semibold)).foregroundStyle(AppTheme.title)
            TextField(project.name, text: displayNameBinding)
                .textFieldStyle(.plain)
                .font(.system(size: 13))
                .padding(8)
                .background(
                    RoundedRectangle(cornerRadius: 6)
                        .fill(AppTheme.filterFieldBg)
                        .overlay(RoundedRectangle(cornerRadius: 6).stroke(AppTheme.hairline, lineWidth: 1))
                )
        }
    .enableInjection()
    }
}

private struct RepoIconSection: View {
    @ObserveInjection private var inject

    @Bindable var model: AppModel
    let project: Project

    private enum IconTab: String, CaseIterable { case avatar = "Avatar", icon = "Icon", emoji = "Emoji" }
    @State private var selectedTab: IconTab = .avatar
    @State private var customColor: Color = .gray

    private static let palette: [(name: String, hex: String)] = [
        ("Red", "#FF3B30"), ("Orange", "#FF9500"), ("Yellow", "#FFCC00"),
        ("Green", "#34C759"), ("Teal", "#30B0C7"), ("Purple", "#AF52DE"), ("Pink", "#FF2D55")
    ]

    private var resolvedColor: Color {
        if let hex = project.colorHex, let color = Color(hex: hex) { return color }
        let palette: [Color] = [.blue, .orange, .green, .purple, .pink, .teal, .indigo, .yellow]
        var hash = 5381
        for byte in project.name.utf8 { hash = (hash &* 33) &+ Int(byte) }
        return palette[abs(hash) % palette.count]
    }

    @ViewBuilder
    private var iconPreview: some View {
        ZStack {
            Circle().fill(resolvedColor.opacity(0.15)).frame(width: 32, height: 32)
            switch project.iconKind {
            case .avatar:
                if let data = project.avatarImage, let nsImage = NSImage(data: data) {
                    Image(nsImage: nsImage)
                        .resizable()
                        .aspectRatio(contentMode: .fill)
                        .frame(width: 32, height: 32)
                        .clipShape(Circle())
                } else {
                    Image(systemName: "folder.fill").foregroundStyle(resolvedColor)
                }
            case .icon:
                Image(systemName: project.iconValue ?? "folder.fill").foregroundStyle(resolvedColor)
            case .emoji:
                if let emoji = project.iconValue, !emoji.isEmpty {
                    Text(emoji).font(.system(size: 16))
                } else {
                    Image(systemName: "folder.fill").foregroundStyle(resolvedColor)
                }
            }
        }
        .frame(width: 32, height: 32)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 10) {
                Text("Repo Icon").font(.system(size: 15, weight: .bold)).foregroundStyle(AppTheme.title)
                iconPreview
                Spacer()
                Button {
                    Task {
                        await model.setProjectColor(project, colorHex: nil)
                        await model.setProjectIcon(project, kind: .icon, value: nil, avatarImage: nil)
                    }
                } label: {
                    Label("Reset", systemImage: "arrow.counterclockwise")
                }
                .buttonStyle(.plain)
                .foregroundStyle(AppTheme.subtitle)
            }

            Text("Color").font(.system(size: 13, weight: .semibold)).foregroundStyle(AppTheme.title)
            HStack(spacing: 10) {
                colorSwatch(fill: Color.gray.opacity(0.5), isSelected: project.colorHex == nil)
                    .onTapGesture {
                        Task { await model.setProjectColor(project, colorHex: nil) }
                    }
                ForEach(Self.palette, id: \.hex) { swatch in
                    colorSwatch(fill: Color(hex: swatch.hex) ?? .gray, isSelected: project.colorHex == swatch.hex)
                        .onTapGesture {
                            Task { await model.setProjectColor(project, colorHex: swatch.hex) }
                        }
                }
                ColorPicker("", selection: $customColor, supportsOpacity: false)
                    .labelsHidden()
                    .frame(width: 22, height: 22)
                    .clipShape(Circle())
                    .onChange(of: customColor) { _, newValue in
                        Task { await model.setProjectColor(project, colorHex: newValue.toHex()) }
                    }
            }

            Picker("", selection: $selectedTab) {
                ForEach(IconTab.allCases, id: \.self) { tab in
                    Text(tab.rawValue).tag(tab)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .frame(maxWidth: 300)

            switch selectedTab {
            case .avatar: AvatarTabContent(model: model, project: project)
            case .icon: IconTabContent(model: model, project: project)
            case .emoji: EmojiTabContent(model: model, project: project)
            }
        }
    .enableInjection()
    }

    private func colorSwatch(fill: Color, isSelected: Bool) -> some View {
        Circle()
            .fill(fill)
            .frame(width: 22, height: 22)
            .overlay(Circle().stroke(AppTheme.title, lineWidth: isSelected ? 2 : 0))
            .contentShape(Circle())
    }
}

private struct AvatarTabContent: View {
    @ObserveInjection private var inject

    @Bindable var model: AppModel
    let project: Project

    @State private var githubOwner: String?
    @State private var faviconURLText = ""
    @State private var isLoading = false
    @State private var errorMessage: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Button {
                fetchGitHubAvatar()
            } label: {
                Label("Use GitHub Avatar", systemImage: "person.crop.circle")
                    .frame(maxWidth: .infinity)
            }
            .disabled(githubOwner == nil || isLoading)
            .help(githubOwner == nil ? "No GitHub remote found" : "Use github.com/\(githubOwner ?? "").png")

            Text("Used by default — GitHub always provides one, even when the owner hasn't set a custom image.")
                .font(.system(size: 11)).foregroundStyle(AppTheme.meta)

            Button {
                uploadPNG()
            } label: {
                Label("Upload PNG", systemImage: "photo")
            }
            .disabled(isLoading)

            HStack {
                TextField("example.com", text: $faviconURLText)
                    .textFieldStyle(.plain)
                    .padding(8)
                    .background(
                        RoundedRectangle(cornerRadius: 6)
                            .fill(AppTheme.filterFieldBg)
                            .overlay(RoundedRectangle(cornerRadius: 6).stroke(AppTheme.hairline, lineWidth: 1))
                    )
                Button("Favicon") { fetchFavicon() }
                    .disabled(faviconURLText.trimmingCharacters(in: .whitespaces).isEmpty || isLoading)
            }

            Text("PNG uploads must be 256KB or smaller.")
                .font(.system(size: 11)).foregroundStyle(AppTheme.meta)

            if let errorMessage {
                Text(errorMessage).font(.system(size: 11)).foregroundStyle(.red)
            }
        }
        .task {
            githubOwner = await GitRemote.githubOwner(repoPath: project.rootPath)
        }
    .enableInjection()
    }

    private func fetchGitHubAvatar() {
        guard let owner = githubOwner, let url = URL(string: "https://github.com/\(owner).png") else { return }
        isLoading = true
        errorMessage = nil
        Task {
            defer { isLoading = false }
            do {
                let (data, _) = try await URLSession.shared.data(from: url)
                await model.setProjectIcon(project, kind: .avatar, value: nil, avatarImage: data)
            } catch {
                errorMessage = "Couldn't fetch GitHub avatar: \(error.localizedDescription)"
            }
        }
    }

    private func uploadPNG() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = false
        panel.canChooseFiles = true
        panel.allowsMultipleSelection = false
        panel.allowedContentTypes = [.png]
        panel.message = "Choose a PNG image"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        guard let data = try? Data(contentsOf: url) else {
            errorMessage = "Couldn't read the selected file."
            return
        }
        guard data.count <= 256 * 1024 else {
            errorMessage = "PNG must be 256KB or smaller (selected file is \(data.count / 1024)KB)."
            return
        }
        errorMessage = nil
        Task { await model.setProjectIcon(project, kind: .avatar, value: nil, avatarImage: data) }
    }

    private func fetchFavicon() {
        var text = faviconURLText.trimmingCharacters(in: .whitespaces)
        if !text.contains("://") { text = "https://" + text }
        guard let components = URLComponents(string: text), let host = components.host,
              let url = URL(string: "https://\(host)/favicon.ico")
        else {
            errorMessage = "Enter a valid domain, e.g. example.com."
            return
        }
        isLoading = true
        errorMessage = nil
        Task {
            defer { isLoading = false }
            do {
                let (data, _) = try await URLSession.shared.data(from: url)
                await model.setProjectIcon(project, kind: .avatar, value: nil, avatarImage: data)
            } catch {
                errorMessage = "Couldn't fetch favicon: \(error.localizedDescription)"
            }
        }
    }
}

private struct IconTabContent: View {
    @ObserveInjection private var inject

    @Bindable var model: AppModel
    let project: Project

    private static let symbols = [
        "folder.fill", "star.fill", "flame.fill", "bolt.fill", "terminal.fill", "gearshape.fill",
        "cube.fill", "leaf.fill", "cloud.fill", "bookmark.fill", "tag.fill", "flag.fill",
        "paperplane.fill", "hammer.fill", "wrench.fill", "puzzlepiece.fill", "server.rack",
        "shippingbox.fill", "archivebox.fill", "doc.fill", "chart.bar.fill", "globe",
        "network", "keyboard"
    ]

    private let columns = Array(repeating: GridItem(.flexible(), spacing: 8), count: 6)

    var body: some View {
        LazyVGrid(columns: columns, spacing: 8) {
            ForEach(Self.symbols, id: \.self) { symbol in
                Button {
                    Task { await model.setProjectIcon(project, kind: .icon, value: symbol, avatarImage: nil) }
                } label: {
                    Image(systemName: symbol)
                        .frame(width: 28, height: 28)
                        .background(
                            RoundedRectangle(cornerRadius: 6)
                                .fill(project.iconKind == .icon && project.iconValue == symbol
                                      ? AppTheme.rowHover : Color.clear)
                        )
                }
                .buttonStyle(.plain)
            }
        }
    .enableInjection()
    }
}

private struct EmojiTabContent: View {
    @ObserveInjection private var inject

    @Bindable var model: AppModel
    let project: Project
    @State private var emojiText: String = ""

    var body: some View {
        HStack(spacing: 10) {
            TextField("🙂", text: $emojiText)
                .textFieldStyle(.plain)
                .font(.system(size: 20))
                .frame(width: 44, height: 32)
                .multilineTextAlignment(.center)
                .background(
                    RoundedRectangle(cornerRadius: 6)
                        .fill(AppTheme.filterFieldBg)
                        .overlay(RoundedRectangle(cornerRadius: 6).stroke(AppTheme.hairline, lineWidth: 1))
                )
                .onChange(of: emojiText) { _, newValue in
                    let single = String(newValue.suffix(1))
                    if single != newValue { emojiText = single }
                    Task {
                        await model.setProjectIcon(
                            project, kind: .emoji, value: single.isEmpty ? nil : single, avatarImage: nil
                        )
                    }
                }
            Button("Open Emoji Picker\u{2026}") {
                NSApp.orderFrontCharacterPalette(nil)
            }
            .buttonStyle(.plain)
            .foregroundStyle(AppTheme.subtitle)
        }
        .onAppear { emojiText = project.iconValue ?? "" }
    .enableInjection()
    }
}

private struct WorktreeBaseSection: View {
    @ObserveInjection private var inject

    @Bindable var model: AppModel
    let project: Project

    @State private var branches: [String] = []
    @State private var searchText = ""

    private var primaryWorktree: Worktree? {
        (model.worktrees[project.id] ?? []).first(where: \.isPrimary)
    }

    private var effectiveBase: String {
        project.defaultWorktreeBase ?? primaryWorktree?.branch ?? "—"
    }

    private var subtitle: String {
        if project.defaultWorktreeBase != nil { return "Pinned" }
        if let branch = primaryWorktree?.branch { return "Following primary branch (\(branch))" }
        return "No primary worktree set"
    }

    private var filteredBranches: [String] {
        let query = searchText.trimmingCharacters(in: .whitespaces)
        guard !query.isEmpty else { return branches }
        return branches.filter { $0.localizedCaseInsensitiveContains(query) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Default Worktree Base").font(.system(size: 15, weight: .bold)).foregroundStyle(AppTheme.title)

            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(effectiveBase).font(.system(size: 13, weight: .semibold)).foregroundStyle(AppTheme.title)
                    Text(subtitle).font(.system(size: 11)).foregroundStyle(AppTheme.subtitle)
                }
                Spacer()
                Button("Use Primary") {
                    Task { await model.setProjectWorktreeBase(project, branch: nil) }
                }
                .buttonStyle(.plain)
                .foregroundStyle(AppTheme.subtitle)
            }

            TextField("Search branches by name\u{2026}", text: $searchText)
                .textFieldStyle(.plain)
                .padding(8)
                .background(
                    RoundedRectangle(cornerRadius: 6)
                        .fill(AppTheme.filterFieldBg)
                        .overlay(RoundedRectangle(cornerRadius: 6).stroke(AppTheme.hairline, lineWidth: 1))
                )

            ForEach(filteredBranches, id: \.self) { branch in
                Text(branch)
                    .font(.system(size: 12.5))
                    .foregroundStyle(AppTheme.title)
                    .padding(.vertical, 4)
                    .padding(.horizontal, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
                    .onTapGesture {
                        Task { await model.setProjectWorktreeBase(project, branch: branch) }
                    }
            }
        }
        .task {
            branches = (try? await GitBranches.list(repoPath: project.rootPath)) ?? []
        }
    .enableInjection()
    }
}

private struct WorktreeLocationSection: View {
    @ObserveInjection private var inject

    @Bindable var model: AppModel
    let project: Project

    private var defaultLocation: String {
        (project.rootPath as NSString).deletingLastPathComponent
    }

    private var locationBinding: Binding<String> {
        Binding(
            get: { project.worktreeLocationOverride ?? "" },
            set: { newValue in
                let trimmed = newValue.trimmingCharacters(in: .whitespaces)
                Task { await model.setProjectWorktreeLocation(project, path: trimmed.isEmpty ? nil : trimmed) }
            }
        )
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Worktree Location").font(.system(size: 15, weight: .bold)).foregroundStyle(AppTheme.title)
            Text("Parent folder for new worktrees. Empty uses the default: \(defaultLocation)")
                .font(.system(size: 11)).foregroundStyle(AppTheme.subtitle)

            HStack {
                TextField(defaultLocation, text: locationBinding)
                    .textFieldStyle(.plain)
                    .padding(8)
                    .background(
                        RoundedRectangle(cornerRadius: 6)
                            .fill(AppTheme.filterFieldBg)
                            .overlay(RoundedRectangle(cornerRadius: 6).stroke(AppTheme.hairline, lineWidth: 1))
                    )
                Button("Choose\u{2026}") { choose() }
                    .buttonStyle(.plain)
            }
        }
    .enableInjection()
    }

    private func choose() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.message = "Choose a folder for new worktrees"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        Task { await model.setProjectWorktreeLocation(project, path: url.path) }
    }
}
