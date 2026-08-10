import Foundation
import AppKit
import SwiftUI
import Testing
import TillerACP
@testable import Tiller

struct ChatRowChromeTests {
    @Test @MainActor func codeInsetFillPreservesTheNearBlackDarkSurface() {
        let fill = NSColor(AppTheme.codeInsetFill).usingColorSpace(.sRGB) ?? .white

        #expect(abs(fill.redComponent - (13.0 / 255.0)) < 0.0001)
        #expect(abs(fill.greenComponent - (14.0 / 255.0)) < 0.0001)
        #expect(abs(fill.blueComponent - (16.0 / 255.0)) < 0.0001)
    }

    @Test func toolLineCountIncludesDiffAndTextContent() {
        let item = ToolCallItem(
            toolCallId: "tool-1",
            title: "Edit Chat.swift",
            kind: .edit,
            status: .completed,
            content: [
                .diff(path: "Chat.swift", oldText: "a\nb", newText: "a\nb\nc"),
                .content(.text("one\ntwo"))
            ])

        #expect(ChatRowMetrics.lineCount(for: item) == 7)
    }

    @Test func truncatedTailLeavesShortTextUntouched() {
        let result = ChatRowMetrics.truncatedTail("one\ntwo\nthree", maxCharacters: 20_000)

        #expect(result.text == "one\ntwo\nthree")
        #expect(result.wasTruncated == false)
    }

    @Test func truncatedTailCapsTextExceedingTheLimit() {
        let huge = String(repeating: "a", count: 100)

        let result = ChatRowMetrics.truncatedTail(huge, maxCharacters: 10)

        #expect(result.text == String(repeating: "a", count: 10))
        #expect(result.wasTruncated == true)
    }

    @Test func truncatedTailCapsASingleLineWithNoNewlines() {
        // MCP tool results commonly arrive as one giant line (e.g. inline
        // JSON) with no "\n" at all — a line-based cap would miss this.
        let hugeSingleLine = String(repeating: "x", count: 50_000)

        let result = ChatRowMetrics.truncatedTail(hugeSingleLine)

        #expect(result.text.count == ChatRowMetrics.maxRenderedCharacters)
        #expect(result.wasTruncated == true)
    }

    @Test func messageMetaCarriesTheTurnTimestamp() {
        let at = Date(timeIntervalSince1970: 1_700_000_000)
        let meta = TimelineRow.MessageMeta(at: at, duration: 3.5,
                                           showsCopyButton: true)

        #expect(meta.at == at)
        #expect(meta.duration == 3.5)
        #expect(meta.showsCopyButton)
    }

    @Test @MainActor func rendersDarkComponentFixture() throws {
        let root = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let output = root
            .appendingPathComponent("artifacts/chat-visual-round-1/tiller-components.png")
        try FileManager.default.createDirectory(
            at: output.deletingLastPathComponent(),
            withIntermediateDirectories: true)

        let renderer = ImageRenderer(content: ChatVisualFixture()
            .frame(width: 760, height: 860)
            .environment(\.colorScheme, .dark))
        renderer.scale = 2
        guard let image = renderer.nsImage,
              let data = image.tiffRepresentation else {
            throw FixtureError.rendererUnavailable
        }
        try data.write(to: output)
    }

    @Test @MainActor func rendersExpandedThinkingFixture() throws {
        let root = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let output = root
            .appendingPathComponent("artifacts/chat-visual-round-2/thinking-expanded.png")
        try FileManager.default.createDirectory(
            at: output.deletingLastPathComponent(),
            withIntermediateDirectories: true)

        let renderer = ImageRenderer(content: ThinkingRowView(
            title: "Inspecting the workspace",
            detail: "Reading the selected files and preparing a focused change.",
            initiallyExpanded: true)
            .frame(width: 760, height: 120)
            .padding(18)
            .background(Color(red: 0.11, green: 0.12, blue: 0.15))
            .environment(\.colorScheme, .dark))
        renderer.scale = 2
        guard let image = renderer.nsImage,
              let data = image.tiffRepresentation else {
            throw FixtureError.rendererUnavailable
        }
        try data.write(to: output)
    }
}

private struct ChatVisualFixture: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
                ChatRowSurface(kind: .user, usesInsetChrome: true) {
                    Text("Summarize the current workspace and suggest one small safe change.")
                        .font(.system(size: 13))
                        .foregroundStyle(AppTheme.title)
                        .textSelection(.enabled)
                }

                ThinkingRowView(title: "Inspecting the workspace",
                                detail: "Reading the selected files and preparing a focused change.")

                ChatRowSurface(kind: .tool, isActive: true, usesInsetChrome: true) {
                    HStack(spacing: 7) {
                        Image(systemName: "chevron.right")
                            .font(.caption2.weight(.semibold))
                        Image(systemName: "doc.text.magnifyingglass")
                        Text("Read AppModel.swift")
                            .font(.callout.weight(.medium))
                        Spacer()
                        ChatCountBadge(count: 18, label: "lines")
                        ChatStatusBadge(status: .completed)
                    }
                }

                MessageRowView(timestamp: Date(timeIntervalSince1970: 1_700_000_000),
                               duration: 12, showsCopyButton: true,
                               copyText: "The selected files share one rendering path.") {
                    Text("The selected files share one rendering path.\n\nI kept the markdown renderer intact and moved the visual state into rows.")
                        .font(.system(size: 14))
                        .foregroundStyle(AppTheme.title)
                }

                ChatRowSurface(kind: .tool, usesInsetChrome: true) {
                    VStack(alignment: .leading, spacing: 0) {
                        HStack(spacing: 7) {
                            HStack(spacing: 5) {
                                Image(systemName: "chevron.left.forwardslash.chevron.right")
                                Text("ChatDiffPreviewView.swift")
                            }
                            .font(.caption.weight(.semibold))
                            Text("Proposal")
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(AppTheme.meta)
                                .padding(.horizontal, 5)
                                .padding(.vertical, 2)
                                .background(AppTheme.primaryPillBg.opacity(0.62),
                                            in: Capsule(style: .continuous))
                            Spacer()
                            Text("+2  −1")
                                .font(.caption2.monospacedDigit())
                                .foregroundStyle(AppTheme.meta)
                        }
                        .padding(.horizontal, 8)
                        .padding(.vertical, 8)
                        .background(AppTheme.cardFill.opacity(0.78))

                        VStack(alignment: .leading, spacing: 1) {
                            DiffFixtureLine(number: 40, sign: " ", text: "let prefix = \"chat\"", tint: AppTheme.subtitle, fill: .clear, isContext: true)
                            DiffFixtureLine(number: 41, sign: "−", text: "let oldTitle = legacyTitle", tint: AppTheme.diffDeletion, fill: AppTheme.diffDeletionBackground)
                            DiffFixtureLine(number: 41, sign: "+", text: "let title = currentTitle", tint: AppTheme.diffAddition, fill: AppTheme.diffAdditionBackground)
                            DiffFixtureLine(number: 42, sign: "+", text: "render(title)", tint: AppTheme.diffAddition, fill: AppTheme.diffAdditionBackground)
                        }
                        .padding(8)
                        .background(AppTheme.codeInsetFill.opacity(0.52),
                                    in: RoundedRectangle(cornerRadius: 6, style: .continuous))
                        .padding(8)
                    }
                    .background(AppTheme.cardFill.opacity(0.74),
                                in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                    .overlay {
                        RoundedRectangle(cornerRadius: 8, style: .continuous)
                            .stroke(AppTheme.hairline.opacity(0.38), lineWidth: 0.5)
                    }
                }

                ChatRowSurface(kind: .tool, usesInsetChrome: true,
                               hoverOverride: true) {
                    HStack(spacing: 7) {
                        Image(systemName: "terminal")
                        Text("Hover state preview")
                            .font(.callout.weight(.medium))
                        Spacer()
                        Text("active + hover")
                            .font(.caption2)
                            .foregroundStyle(AppTheme.meta)
                    }
                }
            }
            .padding(18)
            .frame(maxWidth: .infinity, maxHeight: .infinity,
                   alignment: .topLeading)
            .background(Color(red: 0.11, green: 0.12, blue: 0.15))
    }
}

private struct DiffFixtureLine: View {
    let number: Int
    let sign: String
    let text: String
    let tint: Color
    let fill: Color
    var isContext = false

    var body: some View {
        HStack(spacing: 0) {
            Text(String(format: "%3d", number))
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(AppTheme.meta)
                .frame(width: 30, alignment: .trailing)
                .padding(.trailing, 8)
            Text(sign)
                .font(.system(size: 11, weight: .bold, design: .monospaced))
                .foregroundStyle(tint)
                .frame(width: 14, alignment: .leading)
            Text(text)
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(tint)
            Spacer(minLength: 0)
        }
        .padding(.vertical, 2)
        .background(isContext ? Color.clear : fill.opacity(0.08))
        .overlay(alignment: .leading) {
            if sign != " " {
                Rectangle().fill(tint).frame(width: 2)
            }
        }
    }
}

private enum FixtureError: Error {
    case rendererUnavailable
}
