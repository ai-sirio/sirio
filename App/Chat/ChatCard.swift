import SwiftUI

/// What a chat card represents. Drives the accent rail only — every card
/// shares one fill, one radius, one padding.
enum ChatCardKind {
    case task, question, edit, tool, plan

    var railColor: Color {
        switch self {
        case .task: AppTheme.railTask
        case .question: AppTheme.railQuestion
        case .edit: AppTheme.railEdit
        case .tool, .plan: AppTheme.railTool
        }
    }
}

/// The one place a chat card's appearance is defined. Before this existed the
/// same background modifier was copied into five views and had already drifted
/// to two different corner radii.
struct ChatCard<Content: View>: View {
    let kind: ChatCardKind
    var isHighlighted = false
    @ViewBuilder let content: Content

    private static var radius: CGFloat { 10 }

    var body: some View {
        content
            .padding(.vertical, 10)
            .padding(.leading, 12)
            .padding(.trailing, 12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(AppTheme.cardFill)
            .overlay(alignment: .leading) {
                Rectangle().fill(kind.railColor).frame(width: 3)
            }
            .clipShape(RoundedRectangle(cornerRadius: Self.radius))
            .overlay {
                RoundedRectangle(cornerRadius: Self.radius)
                    .strokeBorder(isHighlighted ? kind.railColor : .clear)
            }
    }
}
