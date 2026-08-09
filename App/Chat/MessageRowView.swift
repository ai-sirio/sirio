import AppKit
import SwiftUI

/// Assistant-message chrome shared by streaming and settled rows.  Content is
/// supplied by the existing markdown view so TextKit remains the rendering
/// authority; this wrapper only owns row rhythm, timestamp and actions.
struct MessageRowView<Content: View>: View {
    let timestamp: Date?
    let duration: TimeInterval?
    let showsCopyButton: Bool
    let copyText: String
    @ViewBuilder let content: Content

    var body: some View {
        // Assistant prose stays in the transcript flow. Keep the shared row
        // hit target and hover behavior, but do not wrap the response in a
        // card or label it as a separate "Agent response" block.
        ChatRowSurface(kind: .assistant, isFlat: true) {
            VStack(alignment: .leading, spacing: 6) {
                content

                if timestamp != nil || duration != nil || showsCopyButton {
                    Rectangle()
                        .fill(AppTheme.hairline.opacity(0.22))
                        .frame(height: 0.5)
                        .padding(.top, 4)
                    HStack(spacing: 8) {
                        if let duration {
                            Text(TranscriptView.formatDuration(duration))
                        }
                        if duration != nil, timestamp != nil {
                            Text("·")
                        }
                        if let timestamp {
                            Text(timestamp, format: .dateTime.hour().minute())
                                .opacity(0.68)
                        }
                        Spacer(minLength: 0)
                        if showsCopyButton {
                            Button {
                                NSPasteboard.general.clearContents()
                                NSPasteboard.general.setString(copyText, forType: .string)
                            } label: {
                                Image(systemName: "doc.on.doc")
                                    .font(AppFont.caption2)
                            }
                            .buttonStyle(.plain)
                            .foregroundStyle(AppTheme.meta)
                            .help("Copy message")
                        }
                    }
                    .font(AppFont.caption2.monospacedDigit())
                    .foregroundStyle(AppTheme.meta.opacity(0.72))
                }
            }
            .padding(.bottom, 4)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
