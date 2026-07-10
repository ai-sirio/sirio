import SwiftUI
import TillerCore
import TillerAgents
import AppKit

struct SidebarView: View {
    @Bindable var model: AppModel
    @State private var newBranchName = ""
    @State private var branchPromptProject: Project?
    @State private var settingsProject: Project?
    @State private var filterText = ""
    @State private var showAddProjectSheet = false

    var body: some View {
        VStack(spacing: 0) {
            FilterField(text: $filterText)
                .padding(.horizontal, 8)
                .padding(.top, 8)
                .padding(.bottom, 4)

            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(filteredProjects) { project in
                        ProjectRow(model: model, project: project, onSettings: { settingsProject = $0 })

                        if model.isProjectExpanded(project) {
                            ForEach(AttentionSort.sorted(model.worktrees[project.id] ?? [], statusOf: model.statusForWorktree)) { worktree in
                                WorktreeRow(model: model, worktree: worktree)
                                    .contextMenu {
                                        Button("Nuovo Terminale") {
                                            model.newShellTab(in: worktree)
                                        }
                                        ForEach(AgentCatalog.all, id: \.id) { adapter in
                                            Button("New \(adapter.displayName) Panel") {
                                                Task { await model.spawnAgent(adapter, in: worktree) }
                                            }
                                        }
                                        Divider()
                                        Button(worktree.isPrimary ? "Unset Primary" : "Set Primary") {
                                            Task { await model.setPrimary(worktree) }
                                        }
                                        Button("Remove Worktree", role: .destructive) {
                                            Task { await model.removeWorktree(worktree) }
                                        }
                                    }

                                ForEach(model.tabs[worktree.id] ?? []) { tab in
                                    TabRow(model: model, worktree: worktree, tab: tab)
                                }
                            }

                            NewWorktreeButton { branchPromptProject = project }
                        }
                    }
                }
                .padding(.horizontal, 8)
                .padding(.top, 4)
                .animation(.easeInOut(duration: 0.18), value: model.expandedProjectIds)
                .animation(.easeInOut(duration: 0.18), value: model.worktrees.mapValues { $0.map(\.id) })
                .animation(.easeInOut(duration: 0.18), value: model.tabs.mapValues { $0.map(\.id) })
            }
            .scrollContentBackground(.hidden)

            Divider().overlay(AppTheme.hairline)
            HStack {
                Button {
                    model.openSettings()
                } label: {
                    Image(systemName: "gearshape")
                }
                .buttonStyle(.plain)
                Button {
                    let readme = URL(fileURLWithPath: "/Users/enzopiopalmisano/orca/projects/orca-mac/README.md")
                    if FileManager.default.fileExists(atPath: readme.path) {
                        NSWorkspace.shared.open(readme)
                    }
                } label: {
                    Image(systemName: "questionmark.circle")
                }
                .buttonStyle(.plain)
                Spacer()
            }
            .foregroundStyle(AppTheme.subtitle)
            .padding(8)
        }
        .toolbar {
            Button {
                showAddProjectSheet = true
            } label: {
                Label("Add Project", systemImage: "plus")
            }
        }
        .alert(
            "New worktree in \(branchPromptProject?.name ?? "")",
            isPresented: .init(
                get: { branchPromptProject != nil },
                set: { if !$0 { branchPromptProject = nil } }
            )
        ) {
            TextField("branch-name", text: $newBranchName)
            Button("Create") {
                if let project = branchPromptProject, !newBranchName.isEmpty {
                    let branch = newBranchName
                    Task { await model.addWorktree(project: project, branch: branch) }
                }
                newBranchName = ""
            }
            Button("Cancel", role: .cancel) { newBranchName = "" }
        }
        .sheet(item: $settingsProject) { project in
            ProjectSettingsSheet(model: model, project: project)
        }
        .sheet(isPresented: $showAddProjectSheet) {
            AddProjectSheet(model: model)
        }
    }

    private var filteredProjects: [Project] {
        model.projects.filter { project in
            SidebarFilter.matches(
                projectName: project.name,
                worktreeBranches: (model.worktrees[project.id] ?? []).map(\.branch),
                query: filterText
            )
        }
    }
}

/// Flat filter box matching the mockup — replaces `.searchable`, whose native
/// rounded pill and blue focus ring clash with the sidebar's flat chrome.
private struct FilterField: View {
    @Binding var text: String

    var body: some View {
        HStack(spacing: 7) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 11))
                .foregroundStyle(AppTheme.meta)
            TextField("Filter", text: $text)
                .textFieldStyle(.plain)
                .font(.system(size: 12.5))
                .foregroundStyle(AppTheme.title)
                .focusEffectDisabled()
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 5)
        .background(
            RoundedRectangle(cornerRadius: 6)
                .fill(AppTheme.filterFieldBg)
                .overlay(
                    RoundedRectangle(cornerRadius: 6)
                        .stroke(AppTheme.hairline, lineWidth: 1)
                )
        )
    }
}

/// Stable per-project tint: explicit colorHex when set, else derived from the
/// name hash so each project keeps its color across launches.
private func projectColor(_ project: Project) -> Color {
    if let hex = project.colorHex, let color = Color(hex: hex) { return color }
    let palette: [Color] = [.blue, .orange, .green, .purple, .pink, .teal, .indigo, .yellow]
    var hash = 5381
    for byte in project.name.utf8 { hash = (hash &* 33) &+ Int(byte) }
    return palette[abs(hash) % palette.count]
}

/// Header di progetto in sidebar: toggle espansione, selezione, icona colorata
/// e badge aggregato dello stato agenti quando il progetto è collassato.
private struct ProjectRow: View {
    @Bindable var model: AppModel
    let project: Project
    let onSettings: (Project) -> Void
    @State private var hovering = false

    var body: some View {
        HStack(spacing: 7) {
            Image(systemName: model.isProjectExpanded(project) ? "chevron.down" : "chevron.right")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(AppTheme.meta)
                .frame(width: 12)
            projectIcon(project)
            Text((project.displayName?.isEmpty == false ? project.displayName : nil) ?? project.name)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(AppTheme.title)
            Spacer(minLength: 4)
            if hovering {
                Button {
                    onSettings(project)
                } label: {
                    Image(systemName: "gearshape")
                        .font(.system(size: 11))
                        .foregroundStyle(AppTheme.meta)
                }
                .buttonStyle(.plain)
            }
            if !model.isProjectExpanded(project), let status = model.statusForProject(project) {
                StatusBadge(status: status)
            }
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 8)
        .contentShape(Rectangle())
        .background(
            RoundedRectangle(cornerRadius: 7)
                .fill(hovering ? AppTheme.rowHover : Color.clear)
        )
        .padding(.top, 2)
        .focusEffectDisabled()
        .onHover { hovering = $0 }
        .onTapGesture {
            model.selectProjectHeader(project)
            model.toggleProjectExpanded(project)
        }
        .contextMenu {
            Button("Remove Project", role: .destructive) {
                Task { await model.removeProject(project) }
            }
        }
    }
}

/// Renders whatever repo icon customization the project has: a cached
/// avatar image, a tinted SF Symbol, or a literal emoji — else the default
/// folder glyph tinted by `projectColor`.
@ViewBuilder
private func projectIcon(_ project: Project) -> some View {
    switch project.iconKind {
    case .avatar:
        if let data = project.avatarImage, let nsImage = NSImage(data: data) {
            Image(nsImage: nsImage)
                .resizable()
                .aspectRatio(contentMode: .fill)
                .frame(width: 14, height: 14)
                .clipShape(Circle())
        } else {
            Image(systemName: "folder.fill")
                .foregroundStyle(projectColor(project))
                .font(.system(size: 13))
        }
    case .icon:
        Image(systemName: project.iconValue ?? "folder.fill")
            .foregroundStyle(projectColor(project))
            .font(.system(size: 13))
    case .emoji:
        if let emoji = project.iconValue, !emoji.isEmpty {
            Text(emoji).font(.system(size: 13))
        } else {
            Image(systemName: "folder.fill")
                .foregroundStyle(projectColor(project))
                .font(.system(size: 13))
        }
    }
}

/// A worktree row, flat unpeel style: a fixed leading status column (animated
/// loader while running, else a lifecycle dot), the branch as title, an
/// optional secondary line (agent + comment), and a trailing relative age.
/// Draws its own selection/hover background — no `List` underneath.
private struct WorktreeRow: View {
    @Bindable var model: AppModel
    let worktree: Worktree
    @State private var hovering = false

    private var isSelected: Bool { model.selectedWorktree?.id == worktree.id }

    var body: some View {
        let status = model.statusForWorktree(worktree)
        let agentId = model.agentIdForWorktree(worktree)
        let comment = worktree.comment ?? ""
        let idle = status == nil && agentId == nil

        HStack(alignment: .center, spacing: 9) {
            WorktreeStatusGlyph(status: status, agentId: agentId)

            VStack(alignment: .leading, spacing: 1) {
                HStack(spacing: 6) {
                    Text(worktree.branch)
                        .font(.system(size: 12.5))
                        .foregroundStyle(isSelected ? AppTheme.titleSelected : AppTheme.title)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    if worktree.isPrimary {
                        Text("primary")
                            .font(.system(size: 9.5))
                            .textCase(.uppercase)
                            .foregroundStyle(AppTheme.title)
                            .padding(.horizontal, 5)
                            .padding(.vertical, 1)
                            .background(Capsule().fill(AppTheme.primaryPillBg))
                    }
                }
                if agentId != nil || !comment.isEmpty {
                    Text(subtitle(agentId: agentId, comment: comment, status: status))
                        .font(.system(size: 11))
                        .foregroundStyle(AppTheme.subtitle)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
            }

            Spacer(minLength: 4)

            if hovering {
                Menu {
                    Button {
                        model.newShellTab(in: worktree)
                    } label: {
                        Label("Nuovo Terminale", systemImage: "terminal")
                    }
                    Divider()
                    ForEach(AgentCatalog.all, id: \.id) { adapter in
                        Button {
                            Task { await model.spawnAgent(adapter, in: worktree) }
                        } label: {
                            if let icon = AgentMenuIconCache.image(for: adapter.id) {
                                Label {
                                    Text(adapter.displayName)
                                } icon: {
                                    Image(nsImage: icon)
                                }
                            } else {
                                Text(adapter.displayName)
                            }
                        }
                    }
                } label: {
                    Image(systemName: "plus")
                        .font(.system(size: 11))
                        .foregroundStyle(AppTheme.meta)
                }
                .buttonStyle(.plain)
                .menuIndicator(.hidden)
                .help("Nuova tab (⌘T)")
            }

            let runningAgentIds = model.runningAgentIds(for: worktree)
            if !runningAgentIds.isEmpty {
                WorktreeRunningAgentsBadge(agentIds: runningAgentIds)
            }

            if !comment.isEmpty, let updated = worktree.commentUpdatedAt {
                Text(Self.relativeAge(updated))
                    .font(.system(size: 11))
                    .foregroundStyle(AppTheme.meta)
            }
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 9)
        .opacity(idle ? 0.5 : 1)
        .contentShape(Rectangle())
        .background(rowBackground)
        .padding(.leading, 14)
        .padding(.vertical, 1)
        .focusEffectDisabled()
        .onHover { hovering = $0 }
        .onTapGesture { model.selectedWorktree = worktree }
    }

    @ViewBuilder private var rowBackground: some View {
        if isSelected {
            RoundedRectangle(cornerRadius: 7)
                .fill(AppTheme.selectionFill)
                .overlay(
                    RoundedRectangle(cornerRadius: 7)
                        .stroke(AppTheme.selectionRing, lineWidth: 1)
                )
        } else if hovering {
            RoundedRectangle(cornerRadius: 7).fill(AppTheme.rowHover)
        } else {
            Color.clear
        }
    }

    private func subtitle(agentId: String?, comment: String, status: AgentStatus?) -> String {
        if !comment.isEmpty {
            if let agentId { return "\(agentId) · \(comment)" }
            return comment
        }
        return status?.humanLabel ?? (agentId ?? "")
    }

    static func relativeAge(_ date: Date, now: Date = Date()) -> String {
        let seconds = Int(now.timeIntervalSince(date))
        switch seconds {
        case ..<60: return "now"
        case ..<3600: return "\(seconds / 60)m"
        case ..<86_400: return "\(seconds / 3600)h"
        default: return "\(seconds / 86_400)d"
        }
    }
}

/// Indented "New Worktree…" affordance shown under an expanded project.
private struct NewWorktreeButton: View {
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        HStack(spacing: 7) {
            Image(systemName: "plus")
                .font(.system(size: 11))
                .frame(width: 18)
            Text("New Worktree\u{2026}")
                .font(.system(size: 12))
            Spacer(minLength: 0)
        }
        .foregroundStyle(hovering ? AppTheme.title : AppTheme.subtitle)
        .padding(.vertical, 5)
        .padding(.horizontal, 9)
        .contentShape(Rectangle())
        .background(
            RoundedRectangle(cornerRadius: 7)
                .fill(hovering ? AppTheme.rowHover : Color.clear)
        )
        .padding(.leading, 14)
        .padding(.vertical, 1)
        .focusEffectDisabled()
        .onHover { hovering = $0 }
        .onTapGesture(perform: action)
    }
}

/// Nodo tab del tree in sidebar: icona (agente / terminale / markdown),
/// titolo, dirty dot per markdown, × in hover, rename inline su doppio click.
/// Indentato sotto la WorktreeRow del proprio worktree.
private struct TabRow: View {
    @Bindable var model: AppModel
    let worktree: Worktree
    let tab: WorkspaceTab
    @State private var hovering = false
    @State private var renaming = false
    @State private var draftTitle = ""
    @FocusState private var renameFieldFocused: Bool

    private var isSelected: Bool {
        model.selectedWorktree?.id == worktree.id
            && model.activeTab(for: worktree.id)?.id == tab.id
    }

    var body: some View {
        HStack(spacing: 7) {
            icon
                .frame(width: 14)

            if renaming {
                TextField("", text: $draftTitle)
                    .textFieldStyle(.plain)
                    .font(.system(size: 12))
                    .focused($renameFieldFocused)
                    .onSubmit {
                        model.renameTab(tab.id, in: worktree.id, to: draftTitle)
                        renaming = false
                    }
                    .onExitCommand { renaming = false }
            } else {
                Text(tab.title)
                    .font(.system(size: 12))
                    .foregroundStyle(isSelected ? AppTheme.titleSelected : AppTheme.subtitle)
                    .lineLimit(1)
                    .truncationMode(.tail)
                if model.markdownDocuments[tab.id]?.isDirty == true {
                    Circle().fill(.secondary).frame(width: 5, height: 5)
                }
            }

            Spacer(minLength: 4)

            if hovering && !renaming {
                Button {
                    model.closeTab(tab.id, in: worktree)
                } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(AppTheme.meta)
                }
                .buttonStyle(.plain)
                .help("Chiudi tab (⌘W)")
            }
        }
        .padding(.vertical, 4)
        .padding(.horizontal, 9)
        .contentShape(Rectangle())
        .background(rowBackground)
        .padding(.leading, 28)
        .padding(.vertical, 1)
        .focusEffectDisabled()
        .onHover { hovering = $0 }
        .onTapGesture(count: 2) {
            draftTitle = tab.title
            renaming = true
            renameFieldFocused = true
        }
        .onTapGesture {
            model.selectedWorktree = worktree
            model.activateTab(tab.id, in: worktree.id)
        }
        .contextMenu {
            Button("Rinomina") {
                draftTitle = tab.title
                renaming = true
                renameFieldFocused = true
            }
            Button("Chiudi") {
                model.closeTab(tab.id, in: worktree)
            }
        }
    }

    @ViewBuilder private var icon: some View {
        if tab.markdownFileURL != nil {
            Image(systemName: "doc.text")
                .font(.system(size: 10))
                .foregroundStyle(AppTheme.meta)
        } else if let agentId = tab.leafIds.compactMap({ model.agentActivity.paneAgents[$0] }).first {
            AgentIcon(agentId: agentId, size: 12)
        } else {
            Image(systemName: "terminal")
                .font(.system(size: 10))
                .foregroundStyle(AppTheme.meta)
        }
    }

    @ViewBuilder private var rowBackground: some View {
        if isSelected {
            RoundedRectangle(cornerRadius: 7)
                .fill(AppTheme.selectionFill)
                .overlay(
                    RoundedRectangle(cornerRadius: 7)
                        .stroke(AppTheme.selectionRing, lineWidth: 1)
                )
        } else if hovering {
            RoundedRectangle(cornerRadius: 7).fill(AppTheme.rowHover)
        } else {
            Color.clear
        }
    }
}

extension Color {
    /// "#RRGGBB" (leading # optional). Returns nil for malformed input.
    init?(hex: String) {
        var value = hex.trimmingCharacters(in: .whitespaces)
        if value.hasPrefix("#") { value.removeFirst() }
        guard value.count == 6, let rgb = UInt32(value, radix: 16) else { return nil }
        self.init(
            red: Double((rgb >> 16) & 0xFF) / 255,
            green: Double((rgb >> 8) & 0xFF) / 255,
            blue: Double(rgb & 0xFF) / 255
        )
    }

    /// "#RRGGBB", best-effort from the color's RGB components (device color
    /// space) — used by the "Custom" swatch in project Repo Icon settings.
    func toHex() -> String? {
        guard let rgb = NSColor(self).usingColorSpace(.deviceRGB) else { return nil }
        let r = Int((rgb.redComponent * 255).rounded())
        let g = Int((rgb.greenComponent * 255).rounded())
        let b = Int((rgb.blueComponent * 255).rounded())
        return String(format: "#%02X%02X%02X", r, g, b)
    }
}

/// Badge di stato agente per l'header di progetto collassato: pallino flat
/// tinto per stato. Nessun Liquid Glass — la sidebar è renderizzata piatta
/// per combaciare col mockup.
private struct StatusBadge: View {
    let status: AgentStatus

    var body: some View {
        Circle()
            .fill(status.badgeColor)
            .frame(width: 7, height: 7)
            .help(status.rawValue)
    }
}
