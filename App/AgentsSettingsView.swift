import SwiftUI
import TillerACP
import Inject

/// Settings → Agents: install/update ACP agents from the official registry.
struct AgentsSettingsView: View {
    @ObserveInjection private var inject

    let center: AcpAgentCenter
    @State private var search = ""

    private var filteredRows: [AcpAgentCenter.AgentRow] {
        guard !search.isEmpty else { return center.rows }
        return center.rows.filter {
            $0.name.localizedCaseInsensitiveContains(search)
                || $0.id.localizedCaseInsensitiveContains(search)
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                TextField("Search agents", text: $search)
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: 260)
                Spacer()
                if let fetched = center.lastFetchedAt {
                    Text("Updated \(fetched.formatted(.relative(presentation: .named)))")
                        .font(AppFont.caption).foregroundStyle(.secondary)
                }
                Button {
                    Task { await center.refresh(force: true) }
                } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .help("Refresh the agent registry")
            }
            if let error = center.registryError {
                Label(error, systemImage: "exclamationmark.triangle")
                    .font(AppFont.caption).foregroundStyle(.orange)
            }
            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(filteredRows) { row in
                        agentRow(row)
                        Divider()
                    }
                }
            }
        }
        .padding(16)
        .task { await center.refresh() }
    .enableInjection()
    }

    @ViewBuilder
    private func agentRow(_ row: AcpAgentCenter.AgentRow) -> some View {
        HStack(spacing: 10) {
            AgentIcon(agentId: row.id, size: 20)
            VStack(alignment: .leading, spacing: 2) {
                Text(row.name).font(AppFont.body)
                if let description = row.description {
                    Text(description).font(AppFont.caption)
                        .foregroundStyle(.secondary).lineLimit(2)
                }
            }
            Spacer()
            statusControl(row)
        }
        .padding(.vertical, 8)
    }

    @ViewBuilder
    private func statusControl(_ row: AcpAgentCenter.AgentRow) -> some View {
        switch center.statuses[row.id] ?? .notInstalled {
        case .notInstalled:
            Button("Install") { Task { await center.install(row.id) } }
                .controlSize(.small)
        case .installing:
            ProgressView().controlSize(.small)
        case .installed(let version):
            Label("Installed \(version)", systemImage: "checkmark.circle.fill")
                .font(AppFont.caption).foregroundStyle(.green)
        case .updateAvailable(let installed, let latest):
            HStack(spacing: 6) {
                Text("v\(installed)").font(AppFont.caption).foregroundStyle(.secondary)
                Button("Update to \(latest)") { Task { await center.install(row.id) } }
                    .controlSize(.small)
            }
        case .failed(let message):
            HStack(spacing: 6) {
                Text(message).font(AppFont.caption).foregroundStyle(.red)
                    .lineLimit(1).help(message)
                Button("Retry") { Task { await center.install(row.id) } }
                    .controlSize(.small)
            }
        case .unsupported:
            Text("Not supported yet").font(AppFont.caption).foregroundStyle(.secondary)
        case .builtin(let available):
            Label(available ? "Available" : "omp binary not found on PATH",
                  systemImage: available ? "checkmark.circle.fill" : "questionmark.circle")
                .font(AppFont.caption)
                .foregroundStyle(available ? .green : .secondary)
        }
    }
}
