import AppKit
import SwiftUI

/// The chip's visual: a rounded box with a symbol and a label, sized to fit.
struct ComposerChipView: View {
    let chip: ComposerChip
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: chip.iconName)
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(.secondary)
            Text(chip.label)
                .font(.caption)
                .lineLimit(1)
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 2)
        .background(
            colorScheme == .dark
                ? AnyShapeStyle(AppTheme.cardFill)
                : AnyShapeStyle(.quaternary.opacity(0.8)),
            in: RoundedRectangle(cornerRadius: 5))
        .overlay(RoundedRectangle(cornerRadius: 5)
            .strokeBorder(.separator.opacity(0.6), lineWidth: 1))
        .fixedSize()
    }
}

/// Hosts `ComposerChipView` inside the text flow.
///
/// TextKit 2 only. Under TextKit 1 compatibility mode AppKit never asks for a
/// view provider, so the chip would silently not render. A guard test in the
/// composer's text layout tests verifies TextKit 2 stays active.
final class ComposerChipViewProvider: NSTextAttachmentViewProvider {
    override func loadView() {
        guard let chip = (textAttachment as? ComposerChipAttachment)?.chip else {
            assertionFailure("ComposerChipViewProvider used with a non-ComposerChipAttachment")
            view = NSView()
            return
        }
        let host = NSHostingView(rootView: ComposerChipView(chip: chip))
        // fittingSize is correct only if ComposerChipView lays out synchronously.
        // Deferred layout (e.g., async image loading) would measure at the wrong size.
        host.frame.size = host.fittingSize
        view = host
        // Let the hosting view's own size drive the attachment's layout
        // bounds, instead of overriding `attachmentBounds`.
        tracksTextAttachmentViewBounds = true
    }
}
