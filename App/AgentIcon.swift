import SwiftUI
import Inject

/// Icon for an agent, visually matching Orca's marks: Claude and Codex are
/// template assets tinted with Orca's colors; OpenCode, Pi and omp are drawn
/// natively from the same vector coordinates Orca uses. Unknown agents fall
/// back to a colored monogram circle.
struct AgentIcon: View {
    @ObserveInjection var inject
    let agentId: String
    var size: CGFloat = 14

    var body: some View {
        Group {
            switch agentId {
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
                OpenCodeLogo()
            case "pi":
                PiLogo()
            case "omp":
                OmpLogo()
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
        .enableInjection()
    }

    /// Orca's Claude brand fill (#D97757).
    static let claudeOrange = Color(red: 0xD9 / 255.0, green: 0x77 / 255.0, blue: 0x57 / 255.0)

    static func color(for id: String) -> Color {
        switch id {
        case "claude": .orange
        case "codex": .green
        case "opencode": .blue
        case "pi": .purple
        case "omp": .teal
        default: .gray
        }
    }
}

/// OpenCode mark (Orca's OpenCodeGoIcon, 240×300 viewBox): a square frame
/// with the lower two thirds of the interior filled dark.
private struct OpenCodeLogo: View {
    @ObserveInjection var inject
    var body: some View {
        GeometryReader { geo in
            let s = min(geo.size.width / 240, geo.size.height / 300)
            let ox = (geo.size.width - 240 * s) / 2
            let oy = (geo.size.height - 300 * s) / 2
            ZStack {
                Path { p in
                    p.addRect(CGRect(x: ox, y: oy, width: 240 * s, height: 300 * s))
                    p.addRect(CGRect(x: ox + 60 * s, y: oy + 60 * s, width: 120 * s, height: 180 * s))
                }
                .fill(Color.primary, style: FillStyle(eoFill: true))
                Path { p in
                    p.addRect(CGRect(x: ox + 60 * s, y: oy + 120 * s, width: 120 * s, height: 120 * s))
                }
                .fill(Color(red: 0x4B / 255.0, green: 0x46 / 255.0, blue: 0x46 / 255.0))
            }
        }
        .enableInjection()
    }
}

/// Pi mark (Orca's PiIcon, 800×800 viewBox), even-odd fill for the counter.
private struct PiLogo: View {
    @ObserveInjection var inject
    var body: some View {
        GeometryReader { geo in
            let s = min(geo.size.width, geo.size.height) / 800
            let ox = (geo.size.width - 800 * s) / 2
            let oy = (geo.size.height - 800 * s) / 2
            func pt(_ x: CGFloat, _ y: CGFloat) -> CGPoint {
                CGPoint(x: ox + x * s, y: oy + y * s)
            }
            return Path { p in
                p.move(to: pt(165.29, 165.29))
                p.addLine(to: pt(517.36, 165.29))
                p.addLine(to: pt(517.36, 400))
                p.addLine(to: pt(400, 400))
                p.addLine(to: pt(400, 517.36))
                p.addLine(to: pt(282.65, 517.36))
                p.addLine(to: pt(282.65, 634.72))
                p.addLine(to: pt(165.29, 634.72))
                p.closeSubpath()
                p.move(to: pt(282.65, 282.65))
                p.addLine(to: pt(282.65, 400))
                p.addLine(to: pt(400, 400))
                p.addLine(to: pt(400, 282.65))
                p.closeSubpath()
                p.move(to: pt(517.36, 400))
                p.addLine(to: pt(634.72, 400))
                p.addLine(to: pt(634.72, 634.72))
                p.addLine(to: pt(517.36, 634.72))
                p.closeSubpath()
            }
            .fill(Color.primary, style: FillStyle(eoFill: true))
        }
        .enableInjection()
    }
}

/// omp mark (omp.sh homepage glyph via Orca's OmpIcon, 64×64 viewBox) with
/// its pink→purple→cyan gradient.
private struct OmpLogo: View {
    @ObserveInjection var inject
    var body: some View {
        GeometryReader { geo in
            let s = min(geo.size.width, geo.size.height) / 64
            let ox = (geo.size.width - 64 * s) / 2
            let oy = (geo.size.height - 64 * s) / 2
            func pt(_ x: CGFloat, _ y: CGFloat) -> CGPoint {
                CGPoint(x: ox + x * s, y: oy + y * s)
            }
            return Path { p in
                p.move(to: pt(10, 14))
                p.addLine(to: pt(54, 14))
                p.addLine(to: pt(54, 23))
                p.addLine(to: pt(43, 23))
                p.addLine(to: pt(43, 56))
                p.addLine(to: pt(34, 56))
                p.addLine(to: pt(34, 23))
                p.addLine(to: pt(25, 23))
                p.addLine(to: pt(25, 45))
                p.addLine(to: pt(16, 45))
                p.addLine(to: pt(16, 23))
                p.addLine(to: pt(10, 23))
                p.closeSubpath()
            }
            .fill(LinearGradient(
                stops: [
                    .init(color: Color(red: 0xED / 255.0, green: 0x4A / 255.0, blue: 0xBF / 255.0), location: 0),
                    .init(color: Color(red: 0x9B / 255.0, green: 0x4D / 255.0, blue: 0xFF / 255.0), location: 0.5),
                    .init(color: Color(red: 0x5A / 255.0, green: 0xD8 / 255.0, blue: 0xE6 / 255.0), location: 1)
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            ))
        }
        .enableInjection()
    }
}
