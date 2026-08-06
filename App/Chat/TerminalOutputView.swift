import SwiftUI
import TillerACP
import Inject

/// Live output of an agent-side terminal command inside a tool call card:
/// monospace tail of the accumulated output plus a running/exit-code chip.
/// The process runs inside the agent — cancelling the turn is the only
/// interruption, so no kill button here.
struct TerminalOutputView: View {
    @ObserveInjection private var inject

    let output: String
    let exit: TerminalExitStatus?
    let isRunning: Bool

    private static let maxLines = 50

    private var tailLines: [Substring] {
        let lines = output.split(separator: "\n", omittingEmptySubsequences: false)
        return Array(lines.suffix(Self.maxLines))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            statusChip
            if !output.isEmpty {
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(Array(tailLines.enumerated()), id: \.offset) { _, line in
                            Text(line.isEmpty ? " " : String(line))
                                .font(.system(size: 12, design: .monospaced))
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                    .textSelection(.enabled)
                    .padding(6)
                }
                .frame(maxHeight: 240)
                .background(Color.black.opacity(0.85),
                            in: RoundedRectangle(cornerRadius: 6))
                .foregroundStyle(Color.white.opacity(0.92))
            }
        }
    .enableInjection()
    }

    @ViewBuilder
    private var statusChip: some View {
        if isRunning {
            Label("in esecuzione", systemImage: "circle.dotted")
                .font(.caption2)
                .foregroundStyle(.orange)
        } else if let exitCode = exit?.exitCode {
            Label("exit \(exitCode)",
                  systemImage: exitCode == 0
                      ? "checkmark.circle" : "xmark.circle")
                .font(.caption2)
                .foregroundStyle(exitCode == 0 ? .green : .red)
        } else if let signal = exit?.signal {
            Label("segnale \(signal)", systemImage: "exclamationmark.triangle")
                .font(.caption2)
                .foregroundStyle(.orange)
        }
    }
}
