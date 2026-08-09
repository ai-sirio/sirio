import AppKit
import SwiftUI
import TillerCore
import TillerWorkspace
import Inject

/// The space tab frames are measured in. The drag coordinator offsets them
/// into workspace-root space, so they must all share one origin. A free-standing
/// constant because the strip is generic over its menu, and a generic type
/// cannot hold a static stored property.
enum PaneTabStripSpace {
    static let name = "paneTabStrip"
}

/// The tab chrome for one pane group, drawn in the app's own theme. It lives
/// here rather than in TillerWorkspace because AppTheme and
/// MainSurfaceMaterial belong to the app target; the package supplies only the
/// model and calls back for the view.
struct PaneTabStripBar<NewTabMenu: View>: View {
    @ObserveInjection private var inject

    @Bindable var model: PaneTabStripModel
    @Bindable var appModel: AppModel
    let workspaceCoordinator: WorkspaceCoordinator
    let worktree: Worktree
    /// The "+" menu's items. Supplied by the caller, which owns AppModel and
    /// knows the worktree this strip belongs to.
    @ViewBuilder var newTabMenu: () -> NewTabMenu

    @State private var escapeMonitor: Any?

    private var resolver: PaneTabPresentationResolver {
        PaneTabPresentationResolver(
            isDirty: workspaceCoordinator.isDocumentDirty,
            liveTerminalPane: {
                workspaceCoordinator.liveControlPaneId(
                    contentID: $0, in: worktree.id)
            },
            browserFaviconURL: { contentID in
                (workspaceCoordinator.adapters[.browser] as? BrowserContentAdapter)?
                    .faviconURL(for: contentID)
            },
            status: { appModel.agentActivity.statusForWorktree(paneIds: $0) },
            agentID: { appModel.agentActivity.agentIdForWorktree(paneIds: $0) })
    }

    var body: some View {
        HStack(spacing: 0) {
            ScrollViewReader { proxy in
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 2) {
                        ForEach(model.entries, id: \.tabID) { entry in
                            let presentation = resolver.resolve(entry)
                            PaneTabStripItem(
                                entry: entry,
                                presentation: presentation,
                                isFocusedGroup: model.isFocusedGroup,
                                worktreePath: worktree.path,
                                onClose: { model.onClose(entry.tabID) },
                                onFrameChange: { model.setTabFrame($0, for: entry.tabID) },
                                onDragChanged: { model.onDragChanged(entry.tabID, $0) },
                                onDragEnded: { model.onDragEnded(entry.tabID) })
                                .frame(
                                    minWidth: PaneTabStripLayout.minimumWidth(isActive: entry.isActive),
                                    maxWidth: PaneTabStripLayout.maximumWidth,
                                    alignment: .leading)
                                .id(entry.tabID)
                        }
                    }
                    .padding(.leading, 6)
                    .onGeometryChange(for: CGFloat.self) { $0.size.width } action: {
                        model.updateContentWidth($0)
                    }
                }
                .onGeometryChange(for: CGFloat.self) { $0.size.width } action: {
                    model.updateViewportWidth($0)
                }
                .onChange(of: model.activeTabID) { _, active in
                    guard let active else { return }
                    withAnimation(.easeInOut(duration: 0.15)) {
                        proxy.scrollTo(active)
                    }
                }
            }

            HStack(spacing: 10) {
                if model.showsOverflowMenu {
                    overflowMenu
                }
                newTabMenuButton
            }
            .padding(.horizontal, 8)
            .frame(height: WorkspaceMetrics.tabStripHeight)
            .background { MainSurfaceMaterial(tint: AppTheme.chatSurface) }
        }
        .frame(height: WorkspaceMetrics.tabStripHeight)
        .coordinateSpace(name: PaneTabStripSpace.name)
        .background { MainSurfaceMaterial(tint: AppTheme.chatSurface) }
        .onChange(of: model.entries.map(\.tabID)) { _, ids in
            model.removeTabFrames(notIn: Set(ids))
        }
        .onAppear {
            escapeMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                // 53 is Escape. The event is passed through either way: with no
                // drag in flight the cancel is a no-op, and swallowing the key
                // would break every other Escape handler in the window.
                if event.keyCode == 53 { model.onDragCancelled() }
                return event
            }
        }
        .onDisappear {
            if let escapeMonitor { NSEvent.removeMonitor(escapeMonitor) }
            escapeMonitor = nil
        }
    .enableInjection()
    }

    private var newTabMenuButton: some View {
        Menu {
            newTabMenu()
        } label: {
            Image(systemName: "plus")
                .font(AppFont.system(size: 11))
                .foregroundStyle(AppTheme.meta)
        }
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .help("New tab (⌘T)")
        .accessibilityLabel("New tab")
    }

    private var overflowMenu: some View {
        Menu {
            ForEach(model.entries, id: \.tabID) { entry in
                let presentation = resolver.resolve(entry)
                Button {
                    model.onActivate(entry.tabID)
                } label: {
                    if entry.isActive {
                        Label(menuTitle(entry, presentation), systemImage: "checkmark")
                    } else {
                        Text(menuTitle(entry, presentation))
                    }
                }
            }
        } label: {
            Image(systemName: "chevron.down")
                .font(AppFont.system(size: 10, weight: .medium))
                .foregroundStyle(AppTheme.meta)
        }
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .help("All tabs")
        .accessibilityLabel("All tabs")
    }

    private func menuTitle(
        _ entry: TabMenuEntry,
        _ presentation: PaneTabPresentation
    ) -> String {
        var parts = ["\(presentation.kindLabel): \(entry.title)"]
        if presentation.isDirty { parts.append("modified") }
        if let status = presentation.agentStatus { parts.append(status.humanLabel) }
        return parts.joined(separator: " — ")
    }
}

private struct PaneTabStripItem: View {
    @ObserveInjection private var inject

    let entry: TabMenuEntry
    let presentation: PaneTabPresentation
    let isFocusedGroup: Bool
    let worktreePath: String
    let onClose: () -> Void
    let onFrameChange: (CGRect) -> Void
    let onDragChanged: (CGPoint) -> Void
    let onDragEnded: () -> Void

    @State private var hovering = false

    var body: some View {
        HStack(spacing: 6) {
            PaneTabIcon(presentation: presentation)
                .frame(width: 14, height: 14)

            Text(entry.title)
                .font(AppFont.system(size: 12))
                .foregroundStyle(entry.isActive ? AppTheme.titleSelected : AppTheme.subtitle)
                .lineLimit(1)
                .truncationMode(.tail)

            if presentation.isDirty {
                Circle().fill(AppTheme.meta).frame(width: 5, height: 5)
            }

            PaneTabStatusGlyph(
                status: presentation.agentStatus,
                agentID: presentation.icon.agentID)

            if hovering {
                Button(action: onClose) {
                    Image(systemName: "xmark")
                        .font(AppFont.system(size: 9, weight: .bold))
                        .foregroundStyle(AppTheme.meta)
                }
                .buttonStyle(HoverIconButtonStyle())
                .frame(
                    width: PaneTabStripLayout.closeControlWidth,
                    height: PaneTabStripLayout.closeControlWidth)
                .help("Close tab (⌘W)")
            } else {
                Color.clear.frame(
                    width: PaneTabStripLayout.closeControlWidth,
                    height: PaneTabStripLayout.closeControlWidth)
            }
        }
        .padding(.horizontal, 9)
        .frame(height: 25)
        .contentShape(Rectangle())
        .background {
            ConnectedPaneTabShape(cornerRadius: 7)
                .fill(entry.isActive ? AppTheme.terminalSurface : (hovering ? AppTheme.rowHover : .clear))
                .overlay {
                    if entry.isActive {
                        ConnectedPaneTabShape(cornerRadius: 7)
                            .stroke(AppTheme.hairline, lineWidth: 1)
                    }
                }
                .overlay(alignment: .top) {
                    if entry.isActive && isFocusedGroup {
                        Capsule()
                            .fill(AppTheme.tabFocusAccent)
                            .frame(height: 2)
                            .padding(.horizontal, 5)
                    }
                }
                .overlay(alignment: .bottom) {
                    if entry.isActive {
                        Rectangle()
                            .fill(AppTheme.terminalSurface)
                            .frame(height: 1)
                    }
                }
        }
        .accessibilityLabel(presentation.accessibilityLabel(for: entry))
        .accessibilityAddTraits(entry.isActive ? .isSelected : [])
        .onGeometryChange(for: CGRect.self) { proxy in
            proxy.frame(in: .named(PaneTabStripSpace.name))
        } action: { frame in
            onFrameChange(frame)
        }
        .onHover { hovering = $0 }
        // One gesture, not a tap plus a drag: a simultaneous DragGesture eats
        // the taps, and DragSession already reports a press that never crossed
        // the threshold as a plain activation.
        //
        // NSEvent.mouseLocation, not the gesture's own location: a SwiftUI drag
        // reports points in this tab's local space, which cannot address the
        // pane next door.
        .gesture(
            DragGesture(minimumDistance: 0)
                .onChanged { _ in onDragChanged(NSEvent.mouseLocation) }
                .onEnded { _ in onDragEnded() }
        )
        .help(tabHelp)
    .enableInjection()
    }

    private var tabHelp: String {
        if case .diff(let documentID) = entry.content {
            let root = URL(fileURLWithPath: worktreePath, isDirectory: true)
                .standardizedFileURL.path
            if documentID.canonicalPath.hasPrefix(root + "/") {
                return String(documentID.canonicalPath.dropFirst(root.count + 1))
            }
            return documentID.canonicalPath
        }
        return entry.title
    }
}

private struct PaneTabIcon: View {
    @ObserveInjection private var inject

    let presentation: PaneTabPresentation

    @ViewBuilder
    var body: some View {
        Group {
            switch presentation.icon {
            case .terminal(let agentID):
                if let agentID {
                    AgentIcon(agentId: agentID, size: 12)
                } else {
                    Image(systemName: "terminal")
                        .font(AppFont.system(size: 10))
                        .foregroundStyle(AppTheme.meta)
                }
            case .chat(let agentID):
                if let agentID {
                    AgentIcon(agentId: agentID, size: 12)
                } else {
                    Image(systemName: "bubble.left")
                        .font(AppFont.system(size: 10))
                        .foregroundStyle(AppTheme.meta)
                }
            case .document(.markdown):
                Image(systemName: "doc.text")
                    .font(AppFont.system(size: 10))
                    .foregroundStyle(AppTheme.meta)
            case .document(.code):
                Image(systemName: "chevron.left.forwardslash.chevron.right")
                    .font(AppFont.system(size: 10))
                    .foregroundStyle(AppTheme.meta)
            case .diff:
                Image(systemName: "arrow.left.arrow.right")
                    .font(AppFont.system(size: 10))
                    .foregroundStyle(AppTheme.meta)
            case .browser(let faviconURL):
                if let faviconURL {
                    AsyncImage(url: faviconURL) { phase in
                        if case .success(let image) = phase { image.resizable() }
                        else { Image(systemName: "globe") }
                    }
                } else {
                    Image(systemName: "globe")
                        .foregroundStyle(AppTheme.meta)
                }
            }
        }
    .enableInjection()
    }
}

private struct ConnectedPaneTabShape: Shape {
    let cornerRadius: CGFloat

    func path(in rect: CGRect) -> Path {
        let radius = min(cornerRadius, rect.width / 2, rect.height)
        var path = Path()
        path.move(to: CGPoint(x: rect.minX, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.minY + radius))
        path.addQuadCurve(
            to: CGPoint(x: rect.minX + radius, y: rect.minY),
            control: CGPoint(x: rect.minX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX - radius, y: rect.minY))
        path.addQuadCurve(
            to: CGPoint(x: rect.maxX, y: rect.minY + radius),
            control: CGPoint(x: rect.maxX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY))
        path.closeSubpath()
        return path
    }
}

@MainActor
func makePaneTabStrip<NewTabMenu: View>(
    _ model: PaneTabStripModel,
    appModel: AppModel,
    workspaceCoordinator: WorkspaceCoordinator,
    worktree: Worktree,
    @ViewBuilder newTabMenu: @escaping () -> NewTabMenu
) -> NSView {
    NSHostingView(rootView: PaneTabStripBar(
        model: model,
        appModel: appModel,
        workspaceCoordinator: workspaceCoordinator,
        worktree: worktree,
        newTabMenu: newTabMenu))
}
