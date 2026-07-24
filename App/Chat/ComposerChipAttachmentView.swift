import AppKit
import SwiftUI

/// The chip's visual: a rounded box with a symbol and a label, sized to fit.
struct ComposerChipView: View {
    let chip: ComposerChip

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
        .background(.quaternary.opacity(0.8), in: RoundedRectangle(cornerRadius: 5))
        .overlay(RoundedRectangle(cornerRadius: 5)
            .strokeBorder(.separator.opacity(0.6), lineWidth: 1))
        .fixedSize()
    }
}

/// Hosts `ComposerChipView` inside the text flow.
///
/// TextKit 2 only. Under TextKit 1 compatibility mode AppKit never asks for a
/// view provider, so the chip would silently not render — see
/// `ComposerTextKit2Tests.measuringHeightKeepsTextKit2`.
final class ComposerChipViewProvider: NSTextAttachmentViewProvider {
    override func loadView() {
        guard let chip = (textAttachment as? ComposerChipAttachment)?.chip else {
            view = NSView()
            return
        }
        let host = NSHostingView(rootView: ComposerChipView(chip: chip))
        host.frame.size = host.fittingSize
        view = host
        // Let the hosting view's own size drive the attachment's layout
        // bounds, instead of overriding `attachmentBounds`.
        tracksTextAttachmentViewBounds = true
    }
}
