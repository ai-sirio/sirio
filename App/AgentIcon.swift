import SwiftUI

/// Icon for an agent, visually matching Orca's marks: Claude and Codex are
/// template assets tinted with Orca's colors; OpenCode, Pi and omp are drawn
/// natively from the same vector coordinates Orca uses. Unknown agents fall
/// back to a colored monogram circle.
struct AgentIcon: View {
    let agentId: String
    var size: CGFloat = 14

    private var normalizedId: String {
        agentId.hasSuffix("-acp") ? String(agentId.dropLast(4)) : agentId
    }

    var body: some View {
        Group {
            switch normalizedId {
            case "claude":
                Image("agent-claude")
                    .renderingMode(.template)
                    .resizable()
                    .scaledToFit()
                    .foregroundStyle(Self.claudeOrange)
            case "codex":
                Image("agent-codex")
                    .renderingMode(.template)
                    .resizable()
                    .scaledToFit()
                    .foregroundStyle(.primary)
            case "opencode":
                ZStack {
                    OpenCodeFrameShape()
                        .fill(Color.primary, style: FillStyle(eoFill: true))
                    OpenCodeInnerShape()
                        .fill(Color(red: 0x4B / 255.0, green: 0x46 / 255.0, blue: 0x46 / 255.0))
                }
            case "pi":
                PiShape()
                    .fill(Color.primary, style: FillStyle(eoFill: true))
            case "omp":
                OmpShape()
                    .fill(LinearGradient(
                        stops: [
                            .init(color: Color(red: 0xED / 255.0, green: 0x4A / 255.0, blue: 0xBF / 255.0), location: 0),
                            .init(color: Color(red: 0x9B / 255.0, green: 0x4D / 255.0, blue: 0xFF / 255.0), location: 0.5),
                            .init(color: Color(red: 0x5A / 255.0, green: 0xD8 / 255.0, blue: 0xE6 / 255.0), location: 1)
                        ],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    ))
            default:
                ZStack {
                    Circle().fill(Self.color(for: agentId))
                    Text(String(agentId.prefix(1)).uppercased())
                        .font(.system(size: size * 0.6, weight: .bold))
                        .foregroundStyle(.white)
                }
            }
        }
        .frame(width: size, height: size)
    }

    /// Orca's Claude brand fill (#D97757).
    static let claudeOrange = Color(red: 0xD9 / 255.0, green: 0x77 / 255.0, blue: 0x57 / 255.0)

    static func color(for id: String) -> Color {
        let normalizedId = id.hasSuffix("-acp") ? String(id.dropLast(4)) : id
        switch normalizedId {
        case "claude": return .orange
        case "codex": return .green
        case "opencode": return .blue
        case "pi": return .purple
        case "omp": return .teal
        default: return .gray
        }
    }
}

/// Maps viewBox coordinates into the target rect, aspect-fit and centered.
/// Shapes (unlike GeometryReader views) have deterministic sizing in every
/// context, including `ImageRenderer` and menu item rasterization.
private struct ViewBoxTransform {
    let scale: CGFloat
    let offsetX: CGFloat
    let offsetY: CGFloat

    init(rect: CGRect, viewBoxWidth: CGFloat, viewBoxHeight: CGFloat) {
        scale = min(rect.width / viewBoxWidth, rect.height / viewBoxHeight)
        offsetX = rect.minX + (rect.width - viewBoxWidth * scale) / 2
        offsetY = rect.minY + (rect.height - viewBoxHeight * scale) / 2
    }

    func point(_ x: CGFloat, _ y: CGFloat) -> CGPoint {
        CGPoint(x: offsetX + x * scale, y: offsetY + y * scale)
    }

    func rect(_ x: CGFloat, _ y: CGFloat, _ w: CGFloat, _ h: CGFloat) -> CGRect {
        CGRect(x: offsetX + x * scale, y: offsetY + y * scale,
               width: w * scale, height: h * scale)
    }
}

/// OpenCode mark, outer frame (Orca's OpenCodeGoIcon, 240×300 viewBox):
/// a square frame drawn via even-odd fill.
private struct OpenCodeFrameShape: Shape {
    func path(in rect: CGRect) -> Path {
        let t = ViewBoxTransform(rect: rect, viewBoxWidth: 240, viewBoxHeight: 300)
        var p = Path()
        p.addRect(t.rect(0, 0, 240, 300))
        p.addRect(t.rect(60, 60, 120, 180))
        return p
    }
}

/// OpenCode mark, lower two thirds of the interior (filled dark).
private struct OpenCodeInnerShape: Shape {
    func path(in rect: CGRect) -> Path {
        let t = ViewBoxTransform(rect: rect, viewBoxWidth: 240, viewBoxHeight: 300)
        return Path(t.rect(60, 120, 120, 120))
    }
}

/// Pi mark (Orca's PiIcon, 800×800 viewBox), even-odd fill for the counter.
private struct PiShape: Shape {
    func path(in rect: CGRect) -> Path {
        let t = ViewBoxTransform(rect: rect, viewBoxWidth: 800, viewBoxHeight: 800)
        var p = Path()
        p.move(to: t.point(165.29, 165.29))
        p.addLine(to: t.point(517.36, 165.29))
        p.addLine(to: t.point(517.36, 400))
        p.addLine(to: t.point(400, 400))
        p.addLine(to: t.point(400, 517.36))
        p.addLine(to: t.point(282.65, 517.36))
        p.addLine(to: t.point(282.65, 634.72))
        p.addLine(to: t.point(165.29, 634.72))
        p.closeSubpath()
        p.move(to: t.point(282.65, 282.65))
        p.addLine(to: t.point(282.65, 400))
        p.addLine(to: t.point(400, 400))
        p.addLine(to: t.point(400, 282.65))
        p.closeSubpath()
        p.move(to: t.point(517.36, 400))
        p.addLine(to: t.point(634.72, 400))
        p.addLine(to: t.point(634.72, 634.72))
        p.addLine(to: t.point(517.36, 634.72))
        p.closeSubpath()
        return p
    }
}

/// omp mark (omp.sh homepage glyph via Orca's OmpIcon, 64×64 viewBox); the
/// caller fills it with the pink→purple→cyan gradient.
private struct OmpShape: Shape {
    func path(in rect: CGRect) -> Path {
        let t = ViewBoxTransform(rect: rect, viewBoxWidth: 64, viewBoxHeight: 64)
        var p = Path()
        p.move(to: t.point(10, 14))
        p.addLine(to: t.point(54, 14))
        p.addLine(to: t.point(54, 23))
        p.addLine(to: t.point(43, 23))
        p.addLine(to: t.point(43, 56))
        p.addLine(to: t.point(34, 56))
        p.addLine(to: t.point(34, 23))
        p.addLine(to: t.point(25, 23))
        p.addLine(to: t.point(25, 45))
        p.addLine(to: t.point(16, 45))
        p.addLine(to: t.point(16, 23))
        p.addLine(to: t.point(10, 23))
        p.closeSubpath()
        return p
    }
}

#Preview {
    HStack(spacing: 12) {
        AgentIcon(agentId: "claude", size: 20)
        AgentIcon(agentId: "codex", size: 20)
        AgentIcon(agentId: "opencode", size: 20)
        AgentIcon(agentId: "pi", size: 20)
        AgentIcon(agentId: "omp", size: 20)
        AgentIcon(agentId: "custom", size: 20)
    }
    .padding()
}
